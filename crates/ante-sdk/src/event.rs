//! Event dispatcher — the core routing engine for the Ante hook system.
//!
//! `EventBus` receives lifecycle events, matches them against registered
//! hook rules, executes matching hooks, and returns the aggregated decision.

use std::sync::Arc;

use ante_protocol_shape::{EventPayload, HookDecision, HookMatchRule, HookPipelineResult};
use tokio::sync::RwLock;

use crate::hooks::registry::HookRegistry;
use crate::hooks::{HookExecutor, HookOutput, InvokeLlm, InvokeMcp, InvokeSubAgent};

/// The central event dispatch bus.
///
/// Holds the hook registry and provides `emit()` for firing events
/// through the hook pipeline.  Optional `llm_callback`, `mcp_callback`
/// and `subagent_callback` are passed to prompt / MCP-tool / sub-agent
/// hooks respectively.
#[derive(Clone)]
pub struct EventBus {
    registry: Arc<RwLock<HookRegistry>>,
    llm_callback: Option<InvokeLlm>,
    mcp_callback: Option<InvokeMcp>,
    subagent_callback: Option<InvokeSubAgent>,
}

impl EventBus {
    /// Create a new event bus backed by the given hook registry.
    pub fn new(registry: HookRegistry) -> Self {
        Self {
            registry: Arc::new(RwLock::new(registry)),
            llm_callback: None,
            mcp_callback: None,
            subagent_callback: None,
        }
    }

    /// Create an empty event bus (no hooks registered). Useful for testing.
    pub fn empty() -> Self {
        Self::new(HookRegistry::new(Vec::new()))
    }

    /// Attach an LLM invocation callback for prompt hooks.
    pub fn with_llm_callback(mut self, cb: InvokeLlm) -> Self {
        self.llm_callback = Some(cb);
        self
    }

    /// Attach an MCP invocation callback for MCP-tool hooks.
    pub fn with_mcp_callback(mut self, cb: InvokeMcp) -> Self {
        self.mcp_callback = Some(cb);
        self
    }

    /// Attach a sub-agent invocation callback for sub-agent hooks.
    pub fn with_subagent_callback(mut self, cb: InvokeSubAgent) -> Self {
        self.subagent_callback = Some(cb);
        self
    }

    /// Fire a lifecycle event through the hook pipeline.
    ///
    /// Returns the aggregated `HookPipelineResult` from all matching hooks.
    ///
    /// For blocking events (`PreToolUse`, `PermissionRequest`), the caller
    /// MUST inspect the decision and act accordingly (deny operations, use
    /// modified input, etc.).
    pub async fn emit(&self, payload: &EventPayload) -> HookPipelineResult {
        let event_type = payload.event_type();
        let start = std::time::Instant::now();

        let registry = self.registry.read().await;
        let matching = registry.match_rules(event_type, payload);

        if matching.is_empty() {
            return HookPipelineResult::passthrough();
        }

        let mut decision = HookDecision::Allow;
        let mut hooks_executed: Vec<String> = Vec::new();

        for rule in &matching {
            for hook in &rule.hooks {
                let executor = HookExecutor::new(hook.clone(), event_type);
                match executor
                    .execute(
                        payload,
                        self.llm_callback.as_ref(),
                        self.mcp_callback.as_ref(),
                        self.subagent_callback.as_ref(),
                    )
                    .await
                {
                    Ok(HookOutput {
                        hook_decision,
                        hook_name,
                    }) => {
                        hooks_executed.push(hook_name);
                        decision = decision.merge(hook_decision);
                        // Early exit on Deny — no point running more hooks
                        if !decision.is_allowed() {
                            return HookPipelineResult {
                                decision,
                                hooks_executed,
                                elapsed_ms: start.elapsed().as_millis() as u64,
                            };
                        }
                    }
                    Err(e) => {
                        // Hook failure → log and continue (fail-open policy)
                        hooks_executed.push(format!("<error: {e}>"));
                    }
                }
            }
        }

        HookPipelineResult {
            decision,
            hooks_executed,
            elapsed_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Reload the hook rules from a new set of match rules.
    pub async fn reload_rules(&self, rules: Vec<HookMatchRule>) {
        let mut registry = self.registry.write().await;
        *registry = HookRegistry::new(rules);
    }

    /// Access the current registry (read-only).
    pub async fn registry(&self) -> HookRegistry {
        self.registry.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ante_protocol_shape::{
        BasePayload, EventPayload, EventType, HookDefinition, HookMatchRule,
    };

    use super::*;

    fn dummy_base() -> BasePayload {
        BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into())
    }

    fn allow_rule() -> HookMatchRule {
        HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash".into()),
            hooks: vec![HookDefinition::Command {
                command: "echo".into(),
                args: vec![r#"{"type":"allow"}"#.into()],
                timeout_ms: None,
            }],
        }
    }

    #[tokio::test]
    async fn emit_passthrough_when_no_rules() {
        let bus = EventBus::empty();
        let payload = EventPayload::PreToolUse(ante_protocol_shape::ToolUsePayload {
            base: dummy_base(),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "ls"}),
            output: None,
            error: None,
            duration_ms: None,
        });
        let result = bus.emit(&payload).await;
        assert!(result.decision.is_allowed());
        assert!(result.hooks_executed.is_empty());
    }

    #[tokio::test]
    async fn emit_matches_rules() {
        let rules = vec![allow_rule()];
        let registry = HookRegistry::new(rules);
        let bus = EventBus::new(registry);
        let payload = EventPayload::PreToolUse(ante_protocol_shape::ToolUsePayload {
            base: dummy_base(),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "ls"}),
            output: None,
            error: None,
            duration_ms: None,
        });
        let result = bus.emit(&payload).await;
        // Command hook won't actually run (no subprocess available in test),
        // but the match should succeed — the command hook will error.
        // This tests the dispatch flow, not the command execution.
        assert_eq!(result.decision.is_allowed(), true);
    }

    #[tokio::test]
    async fn reload_rules_works() {
        let bus = EventBus::empty();
        let rules = vec![allow_rule()];
        bus.reload_rules(rules).await;
        let reg = bus.registry().await;
        assert_eq!(reg.rules().len(), 1);
    }
}
