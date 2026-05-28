//! Human-in-the-loop (HITL) approval system.
//!
//! Intercepts high-risk actions and requests user confirmation
//! before proceeding. Configurable risk thresholds, batching mode,
//! and an "approve everything" mode for trusted sessions.
//!
//! # Modes
//! - `PerRequest`: Each tool call above the risk threshold requires
//!   individual approval (default).
//! - `BatchRiskThreshold`: Tools at or below the configured threshold
//!   are auto-approved. Higher-risk tools are batched for review.
//! - `ApproveAll`: All tool calls are auto-approved. Use with caution.

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use tokio::sync::oneshot;

/// Risk levels for actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// Safe — no approval needed.
    Safe,
    /// Low risk — notify user.
    Low,
    /// Medium risk — ask for confirmation.
    Medium,
    /// High risk — require explicit approval with timeout.
    High,
    /// Critical — block unless explicitly approved.
    Critical,
}

impl RiskLevel {
    /// Whether this risk level requires explicit approval.
    pub fn requires_approval(&self) -> bool {
        matches!(self, RiskLevel::Medium | RiskLevel::High | RiskLevel::Critical)
    }
}

/// Approval mode for the HITL system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitlMode {
    /// Every tool call above the risk threshold requires individual approval.
    PerRequest,
    /// Auto-approve tools at or below the configured risk threshold.
    /// Higher-risk tools are batched for approval.
    BatchRiskThreshold,
    /// Auto-approve everything without user interaction.
    ApproveAll,
}

impl Default for HitlMode {
    fn default() -> Self {
        Self::PerRequest
    }
}

impl HitlMode {
    /// Whether a tool call with the given risk level needs user approval,
    /// given the configured risk threshold.
    pub fn needs_approval(&self, risk: RiskLevel, threshold: RiskLevel) -> bool {
        match self {
            HitlMode::ApproveAll => false,
            HitlMode::BatchRiskThreshold => risk > threshold,
            HitlMode::PerRequest => risk.requires_approval(),
        }
    }

    /// Human-readable name for the mode.
    pub fn as_str(&self) -> &'static str {
        match self {
            HitlMode::PerRequest => "per-request",
            HitlMode::BatchRiskThreshold => "batch-risk-threshold",
            HitlMode::ApproveAll => "approve-all",
        }
    }

    /// Parse from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "per-request" | "per_request" | "perrequest" | "individual" => Some(Self::PerRequest),
            "batch-risk-threshold" | "batch_risk_threshold" | "batch" | "threshold" => Some(Self::BatchRiskThreshold),
            "approve-all" | "approve_all" | "approveall" | "trusted" | "all" => Some(Self::ApproveAll),
            _ => None,
        }
    }
}

/// Compare two RiskLevel values for ordering (Critical > High > Medium > Low > Safe).
impl PartialOrd for RiskLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RiskLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(r: RiskLevel) -> u8 {
            match r {
                RiskLevel::Safe => 0,
                RiskLevel::Low => 1,
                RiskLevel::Medium => 2,
                RiskLevel::High => 3,
                RiskLevel::Critical => 4,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

impl RiskLevel {
    /// Parse a risk level from a string (case-insensitive).
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "safe" => Some(Self::Safe),
            "low" => Some(Self::Low),
            "medium" | "med" => Some(Self::Medium),
            "high" | "h" => Some(Self::High),
            "critical" | "crit" => Some(Self::Critical),
            _ => None,
        }
    }
}

/// An approval request sent to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub action_description: String,
    pub risk_level: RiskLevel,
    pub tool: String,
    pub details: String,
    pub suggested_response: Option<String>,
    pub expires_at: u64, // epoch seconds
}

/// Result of a user approval decision.
///
/// The `Modify` variant carries modified tool‑call arguments that
/// should be used in place of the original inputs. The tool executor
/// checks for this variant and applies the overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
    TimedOut,
    /// User modified the tool‑call arguments before approving.
    /// The inner `Value` is the full transformed arguments map
    /// that the tool executor should use instead of the original.
    Modify(JsonValue),
}

/// Errors from the HITL system.
#[derive(Debug, Error)]
pub enum HitlError {
    #[error("Approval timed out")]
    Timeout,

    #[error("Approval denied by user: {0}")]
    Denied(String),

    #[error("{0}")]
    Internal(String),
}

/// Default approval timeout in seconds per risk level.
fn default_timeout(risk: RiskLevel) -> Duration {
    match risk {
        RiskLevel::Safe | RiskLevel::Low => Duration::from_secs(0),
        RiskLevel::Medium => Duration::from_secs(120),
        RiskLevel::High => Duration::from_secs(60),
        RiskLevel::Critical => Duration::from_secs(30),
    }
}

/// Human-in-the-loop approval manager.
///
/// Supports async approval channels: `request_approval` creates a
/// `oneshot` channel pair. Both sender and receiver are stored
/// internally. The receiver is consumed by `wait_for_approval` which
/// awaits the user's decision. External callers (e.g. a REPL
/// `/approve` command) call `approve()` or `deny()` which send the
/// decision through the channel, waking the waiting task.
pub struct ApprovalManager {
    pending_requests: Vec<ApprovalRequest>,
    custom_timeout: Option<Duration>,
    next_id: u64,
    /// Current approval mode.
    mode: HitlMode,
    /// Maximum risk level that is auto-approved in BatchRiskThreshold mode.
    risk_threshold: RiskLevel,
    /// Pending channels keyed by request ID.
    pending_channels: HashMap<String, PendingApprovalChannel>,
}

/// Internal channel pair for a pending approval request.
struct PendingApprovalChannel {
    /// Wrapped in `Option` so we can `take()` the sender out when
    /// sending a decision without removing the channel entry.
    sender: Option<oneshot::Sender<ApprovalDecision>>,
    /// `None` once `wait_for_approval` has moved out the receiver.
    receiver: Option<oneshot::Receiver<ApprovalDecision>>,
    /// Set when approve/deny/approve_with_modifications has already
    /// sent a decision on the sender but the receiver was already
    /// consumed.  `wait_for_approval` reads this if the receiver is
    /// gone.
    decided: Option<ApprovalDecision>,
}

impl ApprovalManager {
    /// Create a new ApprovalManager with default settings (PerRequest mode).
    pub fn new() -> Self {
        ApprovalManager {
            pending_requests: Vec::new(),
            custom_timeout: None,
            next_id: 0,
            mode: HitlMode::default(),
            risk_threshold: RiskLevel::Low,
            pending_channels: HashMap::new(),
        }
    }

    /// Set a custom timeout. Use Duration::ZERO for no timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.custom_timeout = Some(timeout);
        self
    }

    /// Set the approval mode.
    pub fn with_mode(mut self, mode: HitlMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the risk threshold for BatchRiskThreshold mode.
    pub fn with_risk_threshold(mut self, threshold: RiskLevel) -> Self {
        self.risk_threshold = threshold;
        self
    }

    /// Get the current mode.
    pub fn mode(&self) -> HitlMode {
        self.mode
    }

    /// Get the current risk threshold.
    pub fn risk_threshold(&self) -> RiskLevel {
        self.risk_threshold
    }

    /// Check whether a tool call at the given risk level needs approval.
    pub fn needs_approval(&self, risk: RiskLevel) -> bool {
        self.mode.needs_approval(risk, self.risk_threshold)
    }

    /// Classify a tool call by risk.
    pub fn classify(&self, tool: &str, input: &str) -> RiskLevel {
        let lower = format!("{} {}", tool, input).to_lowercase();

        let critical_patterns = [
            "rm -rf --no-preserve-root", "dd if=",
            "format", "mkfs", "fdisk", "mkswap",
            "> /dev/", "chmod 777 /",
        ];
        if critical_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Critical;
        }

        let high_patterns = [
            "rm -rf", "sudo ", "chmod ", "chown ", "ln -sf",
            "systemctl ", "service ", "apt remove", "dnf remove",
            "curl | bash", "wget -O - | sh", "pip uninstall",
            "drop ", "delete ", "truncate ",
        ];
        if high_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::High;
        }

        let medium_patterns = [
            "write", "edit", "create", "mkdir", "touch",
            "cp ", "mv ", "ln ", "> ", ">>", "tee ",
            "apt install", "dnf install", "pip install", "npm install",
            "git push", "git commit", "git merge", "git rebase",
        ];
        if medium_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Medium;
        }

        let low_patterns = [
            "read", "list", "search", "grep", "find", "globtool",
        ];
        if low_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Low;
        }

        RiskLevel::Safe
    }

    /// Create an approval request and return it.
    ///
    /// Internally creates a `oneshot` channel. The sender is stored
    /// so that `approve()`/`deny()` can signal the decision. The
    /// receiver is stored for `wait_for_approval()` to await.
    pub fn request_approval(
        &mut self,
        action_description: String,
        tool: String,
        details: String,
        risk_level: Option<RiskLevel>,
    ) -> ApprovalRequest {
        self.next_id += 1;
        let id = format!("apr-{:016x}", self.next_id);

        let risk = risk_level.unwrap_or(RiskLevel::High);
        let timeout = self.custom_timeout.unwrap_or_else(|| default_timeout(risk));

        let expires_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + timeout.as_secs();

        let suggested = match risk {
            RiskLevel::Critical => Some("Review carefully — destructive operation".into()),
            RiskLevel::High => Some("Review before approving".into()),
            RiskLevel::Medium => Some("Safe for trusted requests".into()),
            _ => None,
        };

        let request = ApprovalRequest {
            id: id.clone(),
            action_description,
            risk_level: risk,
            tool,
            details,
            suggested_response: suggested,
            expires_at,
        };

        // Create a oneshot channel — sender for approve/deny, receiver for wait_for_approval
        let (tx, rx) = oneshot::channel();
        self.pending_channels.insert(
            id.clone(),
            PendingApprovalChannel {
                sender: Some(tx),
                receiver: Some(rx),
                decided: None,
            },
        );

        self.pending_requests.push(request.clone());
        request
    }

    /// Approve a pending request by ID.
    ///
    /// Sends `Approved` on the oneshot channel, waking the task
    /// blocked on `wait_for_approval`.
    pub fn approve(&mut self, id: &str) -> Result<(), HitlError> {
        let pos = self
            .pending_requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| HitlError::Internal(format!("request {} not found", id)))?;
        self.pending_requests.remove(pos);

        // Send the decision on the channel without removing the entry
        // (wait_for_approval still needs the receiver).  If the
        // receiver was already taken, cache the result.
        if let Some(ch) = self.pending_channels.get_mut(id) {
            if let Some(tx) = ch.sender.take() {
                if tx.send(ApprovalDecision::Approved).is_err() {
                    // Receiver was already consumed — cache
                    ch.decided = Some(ApprovalDecision::Approved);
                }
            }
        }

        Ok(())
    }

    /// Deny a pending request by ID.
    ///
    /// Sends `Denied` on the oneshot channel and returns an error.
    pub fn deny(&mut self, id: &str) -> Result<(), HitlError> {
        let pos = self
            .pending_requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| HitlError::Internal(format!("request {} not found", id)))?;
        let req = self.pending_requests.remove(pos);

        // Send the decision on the channel without removing the entry.
        if let Some(ch) = self.pending_channels.get_mut(id) {
            if let Some(tx) = ch.sender.take() {
                if tx.send(ApprovalDecision::Denied).is_err() {
                    ch.decided = Some(ApprovalDecision::Denied);
                }
            }
        }

        Err(HitlError::Denied(req.action_description))
    }

    /// Approve a pending request with modified tool‑call arguments.
    ///
    /// Sends `Modify(value)` on the oneshot channel. The tool
    /// executor should apply the modified arguments instead of the
    /// original ones.
    pub fn approve_with_modifications(
        &mut self,
        id: &str,
        modified_arguments: JsonValue,
    ) -> Result<(), HitlError> {
        let pos = self
            .pending_requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| HitlError::Internal(format!("request {} not found", id)))?;
        self.pending_requests.remove(pos);

        // Send the Modify decision on the channel.
        if let Some(ch) = self.pending_channels.get_mut(id) {
            if let Some(tx) = ch.sender.take() {
                if tx
                    .send(ApprovalDecision::Modify(modified_arguments.clone()))
                    .is_err()
                {
                    ch.decided = Some(ApprovalDecision::Modify(modified_arguments));
                }
            }
        }

        Ok(())
    }

    /// Wait for the user to approve or deny a request.
    ///
    /// If `approve()` / `deny()` / `approve_with_modifications()` was
    /// already called, the cached decision is returned immediately.
    /// Otherwise awaits the oneshot receiver.  Returns `TimedOut` if
    /// the timeout elapses or the channel is dropped.
    ///
    /// ## Usage
    /// ```ignore
    /// let req = manager.request_approval(…);
    /// let decision = manager.wait_for_approval(&req.id).await;
    /// ```
    pub async fn wait_for_approval(&mut self, id: &str) -> ApprovalDecision {
        // ── Check for a pre-cached decision ──────────────────────────────
        if let Some(d) = self.pending_channels.get(id)
            .and_then(|ch| ch.decided.clone())
        {
            self.pending_channels.remove(id);
            return d;
        }

        // Determine timeout
        let timeout_duration = self.custom_timeout.unwrap_or_else(|| {
            let risk = self
                .pending_requests
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.risk_level)
                .unwrap_or(RiskLevel::High);
            default_timeout(risk)
        });

        // Take the receiver out of the channel entry
        let rx = match self.pending_channels.get_mut(id) {
            Some(ch) => match ch.receiver.take() {
                Some(rx) => rx,
                None => return ApprovalDecision::TimedOut,
            },
            None => return ApprovalDecision::TimedOut,
        };

        // Awaits the decision or timeout
        match tokio::time::timeout(timeout_duration, rx).await {
            Ok(Ok(decision)) => decision,
            _ => {
                self.pending_requests.retain(|r| r.id != id);
                self.pending_channels.remove(id);
                ApprovalDecision::TimedOut
            }
        }
    }

    /// Check if a request has expired.
    pub fn is_expired(&self, request: &ApprovalRequest) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= request.expires_at
    }

    /// Get all pending (non-expired) requests.
    pub fn pending(&self) -> Vec<&ApprovalRequest> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.pending_requests
            .iter()
            .filter(|r| now <= r.expires_at)
            .collect()
    }

    /// Number of pending requests.
    pub fn pending_count(&self) -> usize {
        self.pending().len()
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_action_needs_no_approval() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Read", "read file foo.txt");
        assert!(!risk.requires_approval());
    }

    #[test]
    fn destructive_action_is_critical() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Bash", "rm -rf --no-preserve-root /");
        assert_eq!(risk, RiskLevel::Critical);
    }

    #[test]
    fn rm_is_high_risk() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Bash", "rm -rf /tmp/cache");
        assert_eq!(risk, RiskLevel::High);
    }

    #[test]
    fn sudo_is_high_risk() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Bash", "sudo apt update");
        assert_eq!(risk, RiskLevel::High);
    }

    #[test]
    fn write_is_medium_risk() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Write", "write hello.txt with content");
        assert_eq!(risk, RiskLevel::Medium);
    }

    #[test]
    fn request_and_approve() {
        let mut mgr = ApprovalManager::new();
        let req = mgr.request_approval(
            "delete file".into(),
            "Bash".into(),
            "rm file.txt".into(),
            Some(RiskLevel::Medium),
        );

        assert_eq!(mgr.pending_count(), 1);
        mgr.approve(&req.id).unwrap();
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn deny_returns_error() {
        let mut mgr = ApprovalManager::new();
        let req = mgr.request_approval(
            "danger".into(),
            "Bash".into(),
            "rm -rf /".into(),
            Some(RiskLevel::Critical),
        );

        let result = mgr.deny(&req.id);
        assert!(result.is_err());
        matches!(result, Err(HitlError::Denied(_)));
    }

    #[test]
    fn risk_level_methods() {
        assert!(!RiskLevel::Safe.requires_approval());
        assert!(!RiskLevel::Low.requires_approval());
        assert!(RiskLevel::Medium.requires_approval());
        assert!(RiskLevel::High.requires_approval());
        assert!(RiskLevel::Critical.requires_approval());
    }

    #[test]
    fn classification_case_insensitive() {
        let mgr = ApprovalManager::new();
        let risk = mgr.classify("Bash", "RM -RF /home");
        assert_eq!(risk, RiskLevel::High);
    }

    #[test]
    fn request_has_unique_ids() {
        let mut mgr = ApprovalManager::new();
        let r1 = mgr.request_approval("a".into(), "Bash".into(), "".into(), None);
        let r2 = mgr.request_approval("b".into(), "Bash".into(), "".into(), None);
        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn expired_request_not_pending() {
        let mut mgr = ApprovalManager::new().with_timeout(Duration::from_secs(0));
        let req = mgr.request_approval(
            "test".into(), "Bash".into(), "".into(), Some(RiskLevel::Medium),
        );
        assert!(mgr.is_expired(&req));
    }

    // ─── HitlMode Tests ──────────────────────────────────────────────────

    #[test]
    fn hitl_mode_per_request_needs_approval() {
        assert!(HitlMode::PerRequest.needs_approval(RiskLevel::Medium, RiskLevel::Low));
        assert!(HitlMode::PerRequest.needs_approval(RiskLevel::High, RiskLevel::Low));
        assert!(HitlMode::PerRequest.needs_approval(RiskLevel::Critical, RiskLevel::Low));
        assert!(!HitlMode::PerRequest.needs_approval(RiskLevel::Safe, RiskLevel::Low));
        assert!(!HitlMode::PerRequest.needs_approval(RiskLevel::Low, RiskLevel::Low));
    }

    #[test]
    fn hitl_mode_approve_all_never_needs_approval() {
        assert!(!HitlMode::ApproveAll.needs_approval(RiskLevel::Critical, RiskLevel::Low));
        assert!(!HitlMode::ApproveAll.needs_approval(RiskLevel::Safe, RiskLevel::Low));
        assert!(!HitlMode::ApproveAll.needs_approval(RiskLevel::High, RiskLevel::Low));
    }

    #[test]
    fn hitl_mode_batch_threshold_respects_threshold() {
        // With threshold Low: Safe and Low are auto-approved
        let mode = HitlMode::BatchRiskThreshold;
        let threshold = RiskLevel::Low;

        assert!(!mode.needs_approval(RiskLevel::Safe, threshold));
        assert!(!mode.needs_approval(RiskLevel::Low, threshold));
        assert!(mode.needs_approval(RiskLevel::Medium, threshold));
        assert!(mode.needs_approval(RiskLevel::High, threshold));
        assert!(mode.needs_approval(RiskLevel::Critical, threshold));
    }

    #[test]
    fn hitl_mode_batch_threshold_high_stops_more() {
        let mode = HitlMode::BatchRiskThreshold;
        let threshold = RiskLevel::High;

        assert!(!mode.needs_approval(RiskLevel::Safe, threshold));
        assert!(!mode.needs_approval(RiskLevel::Low, threshold));
        assert!(!mode.needs_approval(RiskLevel::Medium, threshold));
        assert!(!mode.needs_approval(RiskLevel::High, threshold));
        assert!(mode.needs_approval(RiskLevel::Critical, threshold));
    }

    #[test]
    fn hitl_mode_critical_only() {
        let mode = HitlMode::BatchRiskThreshold;
        let threshold = RiskLevel::Critical;

        assert!(!mode.needs_approval(RiskLevel::Critical, threshold));
        assert!(!mode.needs_approval(RiskLevel::Safe, threshold));
    }

    #[test]
    fn hitl_mode_parse_variants() {
        assert_eq!(HitlMode::from_str("per-request"), Some(HitlMode::PerRequest));
        assert_eq!(HitlMode::from_str("per_request"), Some(HitlMode::PerRequest));
        assert_eq!(HitlMode::from_str("PERREQUEST"), Some(HitlMode::PerRequest));
        assert_eq!(HitlMode::from_str("batch"), Some(HitlMode::BatchRiskThreshold));
        assert_eq!(HitlMode::from_str("approve-all"), Some(HitlMode::ApproveAll));
        assert_eq!(HitlMode::from_str("trusted"), Some(HitlMode::ApproveAll));
        assert_eq!(HitlMode::from_str("unknown"), None);
    }

    #[test]
    fn hitl_mode_default_is_per_request() {
        assert_eq!(HitlMode::default(), HitlMode::PerRequest);
    }

    #[test]
    fn approval_manager_mode_affects_needs_approval() {
        let mgr = ApprovalManager::new().with_mode(HitlMode::ApproveAll);
        assert!(!mgr.needs_approval(RiskLevel::Critical));

        let mgr = ApprovalManager::new()
            .with_mode(HitlMode::BatchRiskThreshold)
            .with_risk_threshold(RiskLevel::Medium);
        assert!(!mgr.needs_approval(RiskLevel::Medium));
        assert!(mgr.needs_approval(RiskLevel::High));
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::Safe);
        assert_eq!(RiskLevel::Safe, RiskLevel::Safe);
    }

    #[test]
    fn approve_with_modifications_sends_modified_value() {
        let mut mgr = ApprovalManager::new();
        let req = mgr.request_approval(
            "edit file".into(),
            "Write".into(),
            "write config.json".into(),
            Some(RiskLevel::Medium),
        );

        let modified = serde_json::json!({
            "path": "/safe/config.json",
            "content": "safe content"
        });

        mgr.approve_with_modifications(&req.id, modified.clone())
            .unwrap();
        assert_eq!(mgr.pending_count(), 0);
    }

    #[test]
    fn approve_with_modifications_not_found_errors() {
        let mut mgr = ApprovalManager::new();
        let result = mgr.approve_with_modifications(
            "nonexistent",
            serde_json::json!({"key": "value"}),
        );
        assert!(result.is_err());
    }

}
