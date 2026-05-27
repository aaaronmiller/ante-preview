//! Prompt hook executor — formats a prompt template with event fields,
//! invokes an LLM, and parses the response for a HookDecision.
//!
//! Prompt hooks allow users to define natural-language policies
//! instead of writing shell scripts.

use ante_protocol_shape::{EventPayload, HookDecision};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromptHookError {
    #[error("Prompt hook not yet implemented — request passed through")]
    NotImplemented,
}

/// Execute a prompt hook.
///
/// The prompt template is formatted with event fields and sent to the
/// configured LLM. The response is parsed for a HookDecision.
///
/// # Note
/// This is a placeholder implementation. Prompt hooks require LLM
/// integration which will be wired in a follow-up. For now, all
/// prompt hooks return Allow.
pub async fn run_prompt_hook(
    _prompt_template: &str,
    _model: &Option<String>,
    _payload: &EventPayload,
) -> Result<HookDecision, PromptHookError> {
    // TODO: Implement LLM invocation for prompt hooks
    // 1. Format template with event fields (tool_name, input, event_type, etc.)
    // 2. Send to configured model
    // 3. Parse response for allow/deny/modify
    // 4. Return decision
    //
    // For now, safe default: allow everything.
    Ok(HookDecision::Allow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prompt_hook_defaults_to_allow() {
        use std::path::PathBuf;
        use ante_protocol_shape::ToolUsePayload;

        let payload = EventPayload::PreToolUse(ToolUsePayload {
            base: ante_protocol_shape::BasePayload::new(
                PathBuf::from("/tmp"),
                "0.2.0".into(),
            ),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "ls"}),
            output: None,
            error: None,
            duration_ms: None,
        });

        let result = run_prompt_hook("{{event_type}}", &None, &payload).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_allowed());
    }
}
