//! Hook registry — stores hook match rules and performs matching
//! against incoming events. Includes an O(1) result cache keyed by
//! `(EventType, Option<tool_name>)` that is invalidated whenever
//! the rule set changes.

use std::cell::RefCell;
use std::collections::HashMap;

use ante_protocol_shape::{EventPayload, EventType, HookMatchRule};

/// Cache key: event type + optional tool name.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct MatchCacheKey {
    event_type: EventType,
    tool_name: Option<String>,
}

/// The hook registry holds all configured hook match rules and provides
/// matching logic against lifecycle events.
///
/// Includes a transparent O(1) cache: the first call to `match_rules`
/// for a unique `(event_type, tool_name)` pair computes the matching
/// rules and caches the result. Subsequent calls with the same pair
/// return immediately from the cache. The cache is invalidated when
/// `set_rules()` is called.
#[derive(Debug, Clone)]
pub struct HookRegistry {
    rules: Vec<HookMatchRule>,
    /// Indices into `rules`, cached per `(EventType, Option<tool_name>)`.
    /// Uses `RefCell` for interior mutability so `match_rules` can remain
    /// `&self` (reflection-friendly, ergonomic for event bus pipelines).
    cache: RefCell<HashMap<MatchCacheKey, Vec<usize>>>,
}

impl HookRegistry {
    /// Create a new registry with the given rules.
    pub fn new(rules: Vec<HookMatchRule>) -> Self {
        Self {
            rules,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Create an empty registry (no hooks).
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Find all rules that match the given event and payload.
    ///
    /// A rule matches if:
    /// 1. Its `event_types` includes the given event type, AND
    /// 2. Its `tool_name_pattern` (if set) matches the tool name in the payload.
    ///
    /// Results are cached: subsequent calls with the same event type and
    /// tool name return in O(1) amortized.
    pub fn match_rules<'a>(
        &'a self,
        event_type: EventType,
        payload: &EventPayload,
    ) -> Vec<&'a HookMatchRule> {
        let tool_name = extract_tool_name(payload);
        let key = MatchCacheKey { event_type, tool_name };

        // Check cache first (O(1) lookup)
        {
            let cache = self.cache.borrow();
            if let Some(indices) = cache.get(&key) {
                return indices.iter().map(|&i| &self.rules[i]).collect();
            }
        }

        // Cache miss: compute matching rules
        let matching_indices: Vec<usize> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| {
                // Check event type match
                if !rule.event_types.contains(&event_type) {
                    return false;
                }

                // Check tool name pattern if present
                if let Some(pattern) = &rule.tool_name_pattern {
                    match &key.tool_name {
                        Some(name) => {
                            if !name_matches(pattern, name) {
                                return false;
                            }
                        }
                        None => {
                            return false;
                        }
                    }
                }

                true
            })
            .map(|(i, _)| i)
            .collect();

        // Store in cache (interior mutability via RefCell)
        self.cache.borrow_mut().insert(key, matching_indices.clone());

        // Return references
        matching_indices.iter().map(|&i| &self.rules[i]).collect()
    }

    /// Get a reference to all rules.
    pub fn rules(&self) -> &[HookMatchRule] {
        &self.rules
    }

    /// Replace all rules and invalidate the cache.
    pub fn set_rules(&mut self, rules: Vec<HookMatchRule>) {
        self.rules = rules;
        self.cache.borrow_mut().clear();
    }

    /// Invalidate the match cache without changing the rules.
    pub fn invalidate_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    /// Number of cached entries.
    pub fn cache_size(&self) -> usize {
        self.cache.borrow().len()
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

    // ─── Cache Tests ────────────────────────────────────────────────────

    #[test]
    fn cache_starts_empty() {
        let reg = HookRegistry::empty();
        assert_eq!(reg.cache_size(), 0);
    }

    #[test]
    fn cache_populated_on_miss() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: None,
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");

        // First call — cache miss, result computed
        let _ = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(reg.cache_size(), 1);
    }

    #[test]
    fn cache_hit_returns_same_result() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: None,
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");

        // Two calls with the same key
        let r1 = reg.match_rules(EventType::PreToolUse, &payload);
        let r2 = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 1);
        assert_eq!(r1[0].event_types, r2[0].event_types);
        assert_eq!(reg.cache_size(), 1); // Still only 1 cached entry
    }

    #[test]
    fn cache_distinguishes_tool_names() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash".into()),
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let bp = bash_payload(EventType::PreToolUse, "Bash");
        let wp = bash_payload(EventType::PreToolUse, "Write");

        let r1 = reg.match_rules(EventType::PreToolUse, &bp);
        let r2 = reg.match_rules(EventType::PreToolUse, &wp);
        assert_eq!(r1.len(), 1);
        assert_eq!(r2.len(), 0);
        assert_eq!(reg.cache_size(), 2); // Two separate cache entries
    }

    #[test]
    fn cache_invalidated_on_set_rules() {
        let rule1 = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash".into()),
            hooks: Vec::new(),
        };
        let mut reg = HookRegistry::new(vec![rule1]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");

        // Populate cache
        let _ = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(reg.cache_size(), 1);

        // Replace rules — cache should clear
        let rule2 = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Write".into()),
            hooks: Vec::new(),
        };
        reg.set_rules(vec![rule2]);
        assert_eq!(reg.cache_size(), 0);

        // Should reflect new rules
        let r = reg.match_rules(EventType::PreToolUse, &payload);
        assert_eq!(r.len(), 0); // Bash no longer matches
    }

    #[test]
    fn cache_distinguishes_event_types() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: None,
            hooks: Vec::new(),
        };
        let reg = HookRegistry::new(vec![rule]);
        let payload = bash_payload(EventType::PreToolUse, "Bash");

        let _ = reg.match_rules(EventType::PreToolUse, &payload);
        let _ = reg.match_rules(EventType::PostToolUse, &payload);
        assert_eq!(reg.cache_size(), 2); // Different event types → different cache keys
    }
}
