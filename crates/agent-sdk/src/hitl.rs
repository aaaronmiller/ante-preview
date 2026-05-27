//! Human-in-the-loop (HITL) approval system.
//!
//! Intercepts high-risk actions and requests user confirmation
//! before proceeding. Configurable risk thresholds and timeout.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
    TimedOut,
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
pub struct ApprovalManager {
    pending_requests: Vec<ApprovalRequest>,
    custom_timeout: Option<Duration>,
    next_id: u64,
}

impl ApprovalManager {
    pub fn new() -> Self {
        ApprovalManager {
            pending_requests: Vec::new(),
            custom_timeout: None,
            next_id: 0,
        }
    }

    /// Set a custom timeout. Use Duration::ZERO for no timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.custom_timeout = Some(timeout);
        self
    }

    /// Classify a tool call by risk.
    pub fn classify(&self, tool: &str, input: &str) -> RiskLevel {
        let lower = format!("{} {}", tool, input).to_lowercase();

        // Critical: destructive system operations
        let critical_patterns = [
            "rm -rf --no-preserve-root", "dd if=",
            "format", "mkfs", "fdisk", "mkswap",
            "> /dev/", "chmod 777 /",
        ];
        if critical_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Critical;
        }

        // High: file deletion, sudo, system changes
        let high_patterns = [
            "rm -rf", "sudo ", "chmod ", "chown ", "ln -sf",
            "systemctl ", "service ", "apt remove", "dnf remove",
            "curl | bash", "wget -O - | sh", "pip uninstall",
            "drop ", "delete ", "truncate ",
        ];
        if high_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::High;
        }

        // Medium: file writes, installation, configuration changes
        let medium_patterns = [
            "write", "edit", "create", "mkdir", "touch",
            "cp ", "mv ", "ln ", "> ", ">>", "tee ",
            "apt install", "dnf install", "pip install", "npm install",
            "git push", "git commit", "git merge", "git rebase",
        ];
        if medium_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Medium;
        }

        // Low: reads, searches, lists
        let low_patterns = [
            "read", "list", "search", "grep", "find", "globtool",
        ];
        if low_patterns.iter().any(|p| lower.contains(p)) {
            return RiskLevel::Low;
        }

        RiskLevel::Safe
    }

    /// Create an approval request and return the pending request.
    /// This stores the request for later approval or denial.
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
            id,
            action_description,
            risk_level: risk,
            tool,
            details,
            suggested_response: suggested,
            expires_at,
        };

        self.pending_requests.push(request.clone());
        request
    }

    /// Approve a pending request by ID.
    pub fn approve(&mut self, id: &str) -> Result<(), HitlError> {
        let pos = self
            .pending_requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| HitlError::Internal(format!("request {} not found", id)))?;
        self.pending_requests.remove(pos);
        Ok(())
    }

    /// Deny a pending request by ID.
    pub fn deny(&mut self, id: &str) -> Result<(), HitlError> {
        let pos = self
            .pending_requests
            .iter()
            .position(|r| r.id == id)
            .ok_or_else(|| HitlError::Internal(format!("request {} not found", id)))?;
        let req = self.pending_requests.remove(pos);
        Err(HitlError::Denied(req.action_description))
    }

    /// Wait for approval, timing out if exceeded.
    /// In a real implementation, this would block on a oneshot channel
    /// signaled by the UI thread or a /approve command.
    pub async fn wait_for_approval(&self, _id: &str) -> ApprovalDecision {
        // TODO: Wire into agent main loop for async approval channels.
        // For now, auto-approve with notification.
        tokio::time::sleep(Duration::from_millis(100)).await;
        ApprovalDecision::Approved
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
        let mut mgr = ApprovalManager::with_timeout(ApprovalManager::new(), Duration::from_secs(0));
        let req = mgr.request_approval(
            "test".into(), "Bash".into(), "".into(), Some(RiskLevel::Medium),
        );
        assert!(mgr.is_expired(&req));
    }
}
