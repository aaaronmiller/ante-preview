//! Event payload types for the Ante extensibility system.
//!
//! Each variant of `EventPayload` carries the structured data for its
//! corresponding lifecycle event. These are serialized to JSON and passed
//! through stdin to hook scripts.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::EventType;
use crate::id::Id;

/// Common fields that appear in every event payload.
///
/// Note: `event_type` is not included here because it is
/// provided by the `#[serde(tag)]` on `EventPayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasePayload {
    /// ULID-prefixed event identifier (e.g. `evt_01J...`).
    pub id: Id,
    /// ISO-8601 timestamp of when the event was emitted.
    pub timestamp: DateTime<Utc>,
    /// Current working directory of the agent.
    pub cwd: PathBuf,
    /// Path to the session transcript file, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript_path: Option<PathBuf>,
    /// Ante version string.
    pub ante_version: String,
    /// If this event is fired from within a sub-agent, its ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ante_subagent_id: Option<String>,
    /// Arbitrary key-value metadata for extension use.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl BasePayload {
    pub fn new(cwd: PathBuf, ante_version: String) -> Self {
        Self {
            id: Id::evt(),
            timestamp: Utc::now(),
            cwd,
            transcript_path: None,
            ante_version,
            ante_subagent_id: None,
            metadata: HashMap::new(),
        }
    }
}

// ─── Tool event payloads ────────────────────────────────────────────────────

/// Payload for `PreToolUse` and `PostToolUse` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsePayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The tool name (e.g. "Bash", "Read", "Edit").
    pub tool_name: String,
    /// The tool's input arguments as a JSON object.
    pub input: Value,
    /// For `PostToolUse` / `PostToolUseFailure` — the tool's output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    /// Error message if the tool failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Duration of tool execution in milliseconds (post events only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

// ─── User prompt payloads ───────────────────────────────────────────────────

/// Payload for `PreUserPromptSubmit` and `PostUserPromptSubmit` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPromptPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The raw prompt text from the user.
    pub prompt: String,
    /// The model that will process / processed this prompt.
    pub model: String,
    /// Number of conversation turns so far.
    pub turn_count: u32,
}

// ─── Session lifecycle payloads ─────────────────────────────────────────────

/// Payload for `SessionStart` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStartPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The session ID.
    pub session_id: Id,
    /// The active model at session start.
    pub model: String,
    /// The active provider.
    pub provider: String,
    /// The session's project directory (may differ from cwd).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<PathBuf>,
    /// Project name derived from the working directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
}

/// Payload for `SessionEnd` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// Total input tokens used in the session.
    pub total_input_tokens: u64,
    /// Total output tokens used in the session.
    pub total_output_tokens: u64,
    /// Total cost in USD (approximate).
    pub total_cost_usd: f64,
    /// Session duration in seconds.
    pub duration_secs: u64,
    /// Reason for session end.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ─── Compact / memory payloads ──────────────────────────────────────────────

/// Payload for compact events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// Current number of tokens in context.
    pub current_tokens: u64,
    /// Context budget limit in tokens.
    pub budget_tokens: u64,
    /// Current cost in USD.
    pub current_cost_usd: f64,
    /// Cost budget limit in USD.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_cost_usd: Option<f64>,
}

// ─── Permission / approval payloads ─────────────────────────────────────────

/// Risk level for a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Payload for `PermissionRequest` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRequestPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The tool that triggered the approval request.
    pub tool_name: String,
    /// The full input that was going to be sent to the tool.
    pub input: Value,
    /// Assessed risk level.
    pub risk_level: RiskLevel,
    /// Human-readable explanation of why approval is needed.
    pub message: String,
    /// Whether the user can modify the input before approving.
    pub can_modify: bool,
}

// ─── Sub-agent dispatch payloads ────────────────────────────────────────────

/// Payload for `AntePreSubAgentDispatch` / `AntePostSubAgentDispatch` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The sub-agent's name.
    pub subagent_name: String,
    /// The task assigned to the sub-agent.
    pub task: String,
    /// The model selected for this sub-agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Sub-task result (post-dispatch only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
}

// ─── Model selection payloads ───────────────────────────────────────────────

/// Payload for `AnteModelSelected` events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSelectedPayload {
    #[serde(flatten)]
    pub base: BasePayload,
    /// The task complexity classification.
    pub task_complexity: String,
    /// The previously selected model (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_model: Option<String>,
    /// The newly selected model.
    pub selected_model: String,
    /// Reason for the selection (e.g. "cost_optimization", "capability").
    pub selection_reason: String,
}

// ─── Aggregate event payload ────────────────────────────────────────────────

/// A generic event container for the hook dispatch system.
///
/// This is what gets serialized and passed to hook scripts via stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum EventPayload {
    PreToolUse(ToolUsePayload),
    PostToolUse(ToolUsePayload),
    PostToolUseFailure(ToolUsePayload),
    PreUserPromptSubmit(UserPromptPayload),
    PostUserPromptSubmit(UserPromptPayload),
    SessionStart(SessionStartPayload),
    SessionEnd(SessionEndPayload),
    PreCompact(CompactPayload),
    PostCompact(CompactPayload),
    PermissionRequest(PermissionRequestPayload),
    AntePreSubAgentDispatch(SubAgentPayload),
    AntePostSubAgentDispatch(SubAgentPayload),
    AnteModelSelected(ModelSelectedPayload),
}

impl EventPayload {
    /// Returns the base payload fields shared across all variants.
    pub fn base(&self) -> &BasePayload {
        use EventPayload::*;
        match self {
            PreToolUse(p) | PostToolUse(p) | PostToolUseFailure(p) => &p.base,
            PreUserPromptSubmit(p) | PostUserPromptSubmit(p) => &p.base,
            SessionStart(p) => &p.base,
            SessionEnd(p) => &p.base,
            PreCompact(p) | PostCompact(p) => &p.base,
            PermissionRequest(p) => &p.base,
            AntePreSubAgentDispatch(p) | AntePostSubAgentDispatch(p) => &p.base,
            AnteModelSelected(p) => &p.base,
        }
    }

    /// Returns the event type for this payload.
    pub fn event_type(&self) -> EventType {
        use EventPayload::*;
        match self {
            PreToolUse(_) => EventType::PreToolUse,
            PostToolUse(_) => EventType::PostToolUse,
            PostToolUseFailure(_) => EventType::PostToolUseFailure,
            PreUserPromptSubmit(_) => EventType::PreUserPromptSubmit,
            PostUserPromptSubmit(_) => EventType::PostUserPromptSubmit,
            SessionStart(_) => EventType::SessionStart,
            SessionEnd(_) => EventType::SessionEnd,
            PreCompact(_) => EventType::PreCompact,
            PostCompact(_) => EventType::PostCompact,
            PermissionRequest(_) => EventType::PermissionRequest,
            AntePreSubAgentDispatch(_) => EventType::AntePreSubAgentDispatch,
            AntePostSubAgentDispatch(_) => EventType::AntePostSubAgentDispatch,
            AnteModelSelected(_) => EventType::AnteModelSelected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_base() -> BasePayload {
        BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into())
    }

    #[test]
    fn tool_use_payload_serde() {
        let payload = ToolUsePayload {
            base: test_base(),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "ls"}),
            output: None,
            error: None,
            duration_ms: None,
        };
        let payload = EventPayload::PreToolUse(payload);
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: EventPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.event_type(), EventType::PreToolUse);

        let base = back.base();
        assert_eq!(base.cwd, Path::new("/tmp"));
        assert_eq!(base.ante_version, "0.2.0");
    }

    #[test]
    fn session_end_payload_serde() {
        let payload = SessionEndPayload {
            base: test_base(),
            total_input_tokens: 1000,
            total_output_tokens: 500,
            total_cost_usd: 0.05,
            duration_secs: 3600,
            reason: Some("user_request".into()),
        };
        let payload = EventPayload::SessionEnd(payload);
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: EventPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.event_type(), EventType::SessionEnd);
    }

    #[test]
    fn permission_request_payload_serde() {
        let payload = PermissionRequestPayload {
            base: test_base(),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "rm -rf /"}),
            risk_level: RiskLevel::Critical,
            message: "This command is dangerous".into(),
            can_modify: true,
        };
        let payload = EventPayload::PermissionRequest(payload);
        let json = serde_json::to_string(&payload).expect("serialize");
        let back: EventPayload = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, EventPayload::PermissionRequest(_)));
    }

    #[test]
    fn event_type_tag_roundtrip() {
        let payload = EventPayload::SessionStart(SessionStartPayload {
            base: test_base(),
            session_id: Id::new("ses"),
            model: "claude-4".into(),
            provider: "anthropic".into(),
            project_dir: None,
            project_name: Some("ante-spec".into()),
        });
        let json = serde_json::to_string(&payload).expect("serialize");
        // The `event_type` field should come from the tag
        assert!(json.contains(r#""event_type":"session_start""#));
        let back: EventPayload = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.event_type(), EventType::SessionStart);
    }
}
