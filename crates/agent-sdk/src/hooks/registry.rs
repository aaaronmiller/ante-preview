//! Hook registry — stores hook match rules and performs matching
//! against incoming events.

use ante_protocol_shape::{EventPayload, EventType, HookMatchRule};

/// The hook registry holds all configured hook match rules and provides
/// matching logic against lifecycle events.
#[derive(Debug, Clone)]
pub struct HookRegistry {
    rules: Vec<HookMatchRule>,
}

impl HookRegistry {
    /// Create a new registry with the given rules.
    pub fn new(rules: Vec<HookMatchRule>) -> Self {
        Self { rules }
    }

    /// Create an empty registry (no hooks).
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Find all rules that match the given event and payload.
    ///
    /// A rule matches if:
    /// 1. Its `event_types` includes the given event type, AND
    /// 2. Its `tool_name_pattern` (if set) matches the tool name in the payload.
    pub fn match_rules<'a>(
        &'a self,
        event_type: EventType,
        payload: &EventPayload,
    ) -> Vec<&'a HookMatchRule> {
        let tool_name = extract_tool_name(payload);

        self.rules
            .iter()
            .filter(|rule| {
                // Check event type match
                if !rule.event_types.contains(&event_type) {
                    return false;
                }

                // Check tool name pattern if present
                if let Some(pattern) = &rule.tool_name_pattern {
                    match &tool_name {
                        Some(name) => {
                            // Simple substring/glob matching for now
                            if !name_matches(pattern, &name) {
                                return false;
                            }
                        }
                        None => {
                            // Event has no tool name but rule requires one — skip
                            return false;
                        }
                    }
                }

                true
            })
            .collect()
    }

    /// Get a reference to all rules.
    pub fn rules(&self) -> &[HookMatchRule] {
        &self.rules
    }

    /// Replace all rules.
    pub fn set_rules(&mut self, rules: Vec<HookMatchRule>) {
        self.rules = rules;
    }
}

/// Attempt to extract a tool name from the event payload.
fn extract_tool_name(payload: &EventPayload) -> Option<String> {
    use ante_protocol_shape::EventPayload::*;
    match payload {
        PreToolUse(p) | PostToolUse(p) | PostToolUseFailure(p) => {
            Some(p.tool_name.clone())
        }
        PermissionRequest(p) => Some(p.tool_name.clone()),
        _ => None,
    }
}

/// Simple name matching — supports exact match and prefix wildcard (`*`).
///
/// Examples:
/// - `name_matches("Bash", "Bash")` → true
/// - `name_matches("Bash*", "Bash")` → true
/// - `name_matches("Bash*", "Bash:Write")` → true
/// - `name_matches("Write", "Bash")` → false
fn name_matches(pattern: &str, name: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use ante_protocol_shape::{BasePayload, ToolUsePayload};

    fn bash_payload(_event_type: EventType, tool_name: &str) -> EventPayload {
        EventPayload::PreToolUse(ToolUsePayload {
            base: BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into()),
            tool_name: tool_name.into(),
            input: serde_json::json!({"command": "ls"}),
            output: None,
            error: None,
            duration_ms: None,
        })
    }

    #[test]
    fn empty_registry_matches_nothing() {
        let reg = HookRegistry::empty();
        let payload = bash_payload(EventType::PreToolUse, "Bash");
        let matches = reg.match_rules(EventType::PreToolUse, &payload);
        assert!(matches.is_empty());
    }

    #[test]
    fn match_by_event_type() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: None,
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");
        let matches = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn match_by_tool_name() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash".into()),
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");
        let matches = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(matches.len(), 1);

        let payload2 = bash_payload(EventType::PreToolUse, "Write");
        let matches2 = reg.match_rules(EventType::PreToolUse, &payload2);
        assert!(matches2.is_empty());
    }

    #[test]
    fn match_by_wildcard_pattern() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash*".into()),
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash:Write");
        let matches = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn no_match_wrong_event_type() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PostToolUse],
            tool_name_pattern: None,
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");
        let matches = reg.match_rules(EventType::PreToolUse, &payload);
        assert!(matches.is_empty());
    }

    #[test]
    fn name_matching_basic() {
        assert!(name_matches("Bash", "Bash"));
        assert!(!name_matches("Bash", "Write"));
        assert!(name_matches("Bash*", "Bash:Write"));
        assert!(name_matches("Bash*", "Bash"));
        assert!(!name_matches("Write*", "Bash"));
    }
}
