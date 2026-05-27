//! Extensible lifecycle event types for the Ante hook system.
//!
//! These mirror the Claude Code event schema for hook portability
//! while adding Ante-specific variants under the `ante_` prefix.

use serde::{Deserialize, Serialize};

/// All recognized lifecycle event types in the Ante agent loop.
///
/// Each variant corresponds to a point in the agent lifecycle where
/// registered hooks can fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    // ── Pre-execution hooks (blocking — agent waits for decision) ──────────
    /// Fired before a tool is invoked. Hook can allow, deny, or modify the input.
    PreToolUse,
    /// Fired before a user prompt is processed. Hook can modify the prompt.
    PreUserPromptSubmit,

    // ── Post-execution hooks (async — agent does not wait) ─────────────────
    /// Fired after a tool completes successfully.
    PostToolUse,
    /// Fired after a tool fails (non-zero exit, crash, timeout).
    PostToolUseFailure,
    /// Fired after a user prompt is submitted to the LLM.
    PostUserPromptSubmit,

    // ── Session lifecycle ──────────────────────────────────────────────────
    /// Fired when a session starts.
    SessionStart,
    /// Fired when a session ends.
    SessionEnd,

    // ── Compact / memory ───────────────────────────────────────────────────
    /// Fired before context compaction.
    PreCompact,
    /// Fired after context compaction.
    PostCompact,

    // ── Safety / approval ──────────────────────────────────────────────────
    /// Fired when a sensitive operation needs user approval.
    PermissionRequest,

    // ── Ante-specific extensions ───────────────────────────────────────────
    /// Fired when a sub-agent task is about to be dispatched.
    #[serde(rename = "ante_pre_sub_agent_dispatch")]
    AntePreSubAgentDispatch,
    /// Fired after a sub-agent task completes.
    #[serde(rename = "ante_post_sub_agent_dispatch")]
    AntePostSubAgentDispatch,
    /// Fired when the model router selects a new model.
    #[serde(rename = "ante_model_selected")]
    AnteModelSelected,
}

impl EventType {
    /// Returns `true` if this event type is a blocking hook
    /// (agent must wait for the hook decision before proceeding).
    pub fn is_blocking(self) -> bool {
        matches!(
            self,
            EventType::PreToolUse
                | EventType::PreUserPromptSubmit
                | EventType::PermissionRequest
        )
    }

    /// Returns `true` if this event type is an Ante-specific extension
    /// not found in the Claude Code hook schema.
    pub fn is_ante_extension(self) -> bool {
        matches!(
            self,
            EventType::AntePreSubAgentDispatch
                | EventType::AntePostSubAgentDispatch
                | EventType::AnteModelSelected
        )
    }

    /// Convert to the Claude Code event name string (if applicable).
    pub fn claude_code_name(self) -> Option<&'static str> {
        match self {
            EventType::PreToolUse => Some("PreToolUse"),
            EventType::PostToolUse => Some("PostToolUse"),
            EventType::PostToolUseFailure => Some("PostToolUseFailure"),
            EventType::PreUserPromptSubmit => Some("PreUserPromptSubmit"),
            EventType::PostUserPromptSubmit => Some("PostUserPromptSubmit"),
            EventType::SessionStart => Some("SessionStart"),
            EventType::SessionEnd => Some("SessionEnd"),
            EventType::PreCompact => Some("PreCompact"),
            EventType::PostCompact => Some("PostCompact"),
            EventType::PermissionRequest => Some("PermissionRequest"),
            // Ante-specific — no Claude Code equivalent
            _ => None,
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}"));
        write!(f, "{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_serde_roundtrip() {
        for ty in [
            EventType::PreToolUse,
            EventType::PostToolUse,
            EventType::PermissionRequest,
            EventType::AntePreSubAgentDispatch,
        ] {
            let json = serde_json::to_string(&ty).expect("serialize");
            let back: EventType = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(ty, back, "roundtrip failed for {ty:?}");
        }
    }

    #[test]
    fn blocking_events() {
        assert!(EventType::PreToolUse.is_blocking());
        assert!(EventType::PermissionRequest.is_blocking());
        assert!(!EventType::PostToolUse.is_blocking());
        assert!(!EventType::SessionEnd.is_blocking());
    }

    #[test]
    fn ante_extension_renames() {
        let json = serde_json::to_string(&EventType::AntePreSubAgentDispatch)
            .expect("serialize");
        assert_eq!(json, "\"ante_pre_sub_agent_dispatch\"");
    }

    #[test]
    fn claude_code_name_mapping() {
        assert_eq!(EventType::PreToolUse.claude_code_name(), Some("PreToolUse"));
        assert_eq!(
            EventType::AntePreSubAgentDispatch.claude_code_name(),
            None
        );
    }
}
