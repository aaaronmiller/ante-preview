//! Hook decision types — the result of running a hook pipeline.
//!
//! Every hook returns a `HookDecision`. The event dispatcher aggregates
//! decisions from all matching hooks and returns the final result to the
//! agent loop.

use serde::{Deserialize, Serialize};

/// The outcome of a single hook execution.
///
/// For `PreToolUse` and `PermissionRequest` events, the first non-Allow
/// decision wins (deny overrides allow). For post-event hooks, the
/// decision is logged but does not affect execution flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookDecision {
    /// The operation is allowed to proceed as-is.
    Allow,
    /// The operation is denied with an optional reason.
    Deny {
        /// Human-readable explanation of why the operation was denied.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The operation is allowed but with modified inputs.
    Modify {
        /// The modified input payload (tool arguments, prompt text, etc.).
        modified_input: serde_json::Value,
        /// Optional explanation for the modification.
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl HookDecision {
    /// Returns `true` if the operation is allowed (Allow or Modify).
    pub fn is_allowed(&self) -> bool {
        matches!(self, HookDecision::Allow | HookDecision::Modify { .. })
    }

    /// Returns the effective input payload — either the modified value
    /// or `None` if the decision is Allow (caller should use original input).
    pub fn modified_input(&self) -> Option<&serde_json::Value> {
        match self {
            HookDecision::Modify { modified_input, .. } => Some(modified_input),
            _ => None,
        }
    }

    /// Extract the reason message if present.
    pub fn reason(&self) -> Option<&str> {
        match self {
            HookDecision::Deny { reason } | HookDecision::Modify { reason, .. } => {
                reason.as_deref()
            }
            _ => None,
        }
    }

    /// Merge two decisions: more restrictive wins (Deny > Modify > Allow).
    ///
    /// Used when aggregating decisions from multiple hooks.
    pub fn merge(self, other: HookDecision) -> HookDecision {
        use HookDecision::*;
        match (self, other) {
            // Deny always wins
            (Deny { reason }, _) => Deny { reason },
            (_, Deny { reason }) => Deny { reason },
            // Modify + Allow = Modify
            (Modify { modified_input, reason }, Allow) => Modify { modified_input, reason },
            (Allow, Modify { modified_input, reason }) => Modify { modified_input, reason },
            // Two modifies — first one wins (but combine reasons)
            (Modify { modified_input, reason: r1 }, Modify { reason: r2, .. }) => {
                let reason = match (r1, r2) {
                    (Some(a), Some(b)) => Some(format!("{a}; {b}")),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                Modify { modified_input, reason }
            }
            // Allow + Allow = Allow
            (Allow, Allow) => Allow,
        }
    }
}

impl Default for HookDecision {
    fn default() -> Self {
        HookDecision::Allow
    }
}

/// The final aggregated decision from a hook pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPipelineResult {
    /// The effective decision.
    pub decision: HookDecision,
    /// List of hooks that ran, in order.
    pub hooks_executed: Vec<String>,
    /// Total execution time in milliseconds.
    pub elapsed_ms: u64,
}

impl HookPipelineResult {
    /// Create a new result with a single decision and no hooks (passthrough).
    pub fn passthrough() -> Self {
        Self {
            decision: HookDecision::Allow,
            hooks_executed: Vec::new(),
            elapsed_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decision_serde_allow() {
        let d = HookDecision::Allow;
        let json = serde_json::to_string(&d).expect("serialize");
        assert_eq!(json, r#"{"type":"allow"}"#);
        let back: HookDecision = serde_json::from_str(&json).expect("deserialize");
        assert!(back.is_allowed());
    }

    #[test]
    fn decision_serde_deny() {
        let d = HookDecision::Deny { reason: Some("blocked by policy".into()) };
        let json = serde_json::to_string(&d).expect("serialize");
        let back: HookDecision = serde_json::from_str(&json).expect("deserialize");
        assert!(!back.is_allowed());
        assert_eq!(back.reason(), Some("blocked by policy"));
    }

    #[test]
    fn decision_serde_modify() {
        let d = HookDecision::Modify {
            modified_input: json!({"command": "ls -la"}),
            reason: None,
        };
        let json = serde_json::to_string(&d).expect("serialize");
        let back: HookDecision = serde_json::from_str(&json).expect("deserialize");
        assert!(back.is_allowed());
        assert_eq!(
            back.modified_input().and_then(|v| v.get("command")).and_then(|v| v.as_str()),
            Some("ls -la")
        );
    }

    #[test]
    fn merge_deny_wins() {
        let a = HookDecision::Allow;
        let b = HookDecision::Deny { reason: Some("nope".into()) };
        let merged = a.merge(b);
        assert!(!merged.is_allowed());
        assert_eq!(merged.reason(), Some("nope"));
    }

    #[test]
    fn merge_modify_allow() {
        let a = HookDecision::Modify {
            modified_input: json!({"key": "value1"}),
            reason: None,
        };
        let b = HookDecision::Allow;
        let merged = a.merge(b);
        assert!(merged.is_allowed());
        assert_eq!(
            merged.modified_input().and_then(|v| v.get("key")).and_then(|v| v.as_str()),
            Some("value1")
        );
    }
}
