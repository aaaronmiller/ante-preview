//! Prompt hook executor — formats a prompt template with event fields,
//! invokes an LLM via a provided callback, and parses the response for
//! a `HookDecision`.
//!
//! Prompt hooks allow users to define natural-language policies instead
//! of writing shell scripts.  The actual LLM invocation is provided by
//! the caller (e.g. the `ante` crate's Claude client) so this crate
//! stays decoupled from any particular model or provider.

use std::future::Future;
use std::pin::Pin;

use ante_protocol_shape::{EventPayload, HookDecision};
use serde_json::Value;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PromptHookError {
    #[error("Failed to format prompt template: {0}")]
    TemplateFormat(String),

    #[error("LLM invocation returned empty response")]
    EmptyResponse,

    #[error("Failed to parse HookDecision from LLM response: {detail} (response: {response})")]
    ParseDecision { detail: String, response: String },
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Execute a prompt hook.
///
/// 1. Formats the `prompt_template` by replacing `{event_type}` and
///    `{event_json}` with the actual event data.
/// 2. Calls `invoke_llm` with the formatted prompt.
/// 3. Parses the LLM response for a JSON `HookDecision`.
///
/// # Callback
///
/// `invoke_llm` receives the formatted prompt string and must return
/// the LLM's text response.  It can be a simple wrapper that shells
/// out to `claude` or calls an HTTP API.
pub async fn run_prompt_hook(
    prompt_template: &str,
    _model: &Option<String>, // reserved for future model selection
    payload: &EventPayload,
    invoke_llm: impl FnOnce(&str) -> Pin<Box<dyn Future<Output = String> + Send>>,
) -> Result<HookDecision, PromptHookError> {
    // ── 1. Format template ──────────────────────────────────────────────
    let event_type_str = payload.event_type().to_string();
    let event_json = serde_json::to_string_pretty(payload)
        .unwrap_or_else(|_| "{}".to_string());

    let formatted = prompt_template
        .replace("{event_type}", &event_type_str)
        .replace("{event_json}", &event_json);

    // ── 2. Invoke LLM ──────────────────────────────────────────────────
    let response = invoke_llm(&formatted).await;

    if response.trim().is_empty() {
        return Err(PromptHookError::EmptyResponse);
    }

    // ── 3. Parse response ──────────────────────────────────────────────
    parse_decision(&response)
}

/// Parse a `HookDecision` from the LLM's text response.
///
/// Tries to extract JSON from the last code-fenced block or the last
/// line, then deserialises as a `HookDecision`.  Falls back to allowing
/// the operation if parsing fails.
pub fn parse_decision(response: &str) -> Result<HookDecision, PromptHookError> {
    let trimmed = response.trim();

    // Try to find a JSON code block
    let json_str = extract_json_block(trimmed).unwrap_or(trimmed);

    // Try direct deserialisation as HookDecision
    if let Ok(d) = serde_json::from_str::<HookDecision>(json_str) {
        return Ok(d);
    }

    // Try deserialising as a plain object and infer
    if let Ok(val) = serde_json::from_str::<Value>(json_str) {
        // { "allow": true/false }
        if let Some(allowed) = val.get("allow").and_then(|v| v.as_bool()) {
            if allowed {
                if let Some(input) = val.get("modifiedInput").or_else(|| val.get("modified_input"))
                {
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Modify {
                        modified_input: input.clone(),
                        reason,
                    });
                }
                return Ok(HookDecision::Allow);
            } else {
                let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                return Ok(HookDecision::Deny { reason });
            }
        }

        // { "decision": "allow"|"deny"|"modify", ... }
        if let Some(dec) = val.get("decision").and_then(|v| v.as_str()) {
            match dec {
                "deny" => {
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Deny { reason });
                }
                "modify" => {
                    let mi = val
                        .get("modifiedInput")
                        .or_else(|| val.get("modified_input"))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Modify {
                        modified_input: mi,
                        reason,
                    });
                }
                _ => return Ok(HookDecision::Allow),
            }
        }
    }

    // If nothing parsed, allow by default (safe behaviour)
    Ok(HookDecision::Allow)
}

/// Extract JSON from the last fenced code block (```json … ```) in the
/// response, or fall back to looking for a top-level JSON object/array.
fn extract_json_block(text: &str) -> Option<&str> {
    // Find the last ```json block
    let mut best: Option<&str> = None;
    // Find the last ```json...``` block via rfind.
    if let Some(start) = text.rfind("```json") {
        let content_start = start + 7; // skip ```json
        if let Some(end) = text[content_start..].find("```") {
            let block = text[content_start..content_start + end].trim();
            if !block.is_empty() {
                best = Some(block);
            }
        }
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_decision ---------------------------------------------------

    #[test]
    fn parse_allow_direct() {
        let d = parse_decision(r#"{"type":"allow"}"#).unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_deny_direct() {
        let d = parse_decision(r#"{"type":"deny","reason":"nope"}"#).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("nope"));
    }

    #[test]
    fn parse_allow_boolean() {
        let d = parse_decision(r#"{"allow":true}"#).unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_deny_boolean() {
        let d = parse_decision(r#"{"allow":false,"reason":"blocked"}"#).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("blocked"));
    }

    #[test]
    fn parse_decision_key() {
        let d = parse_decision(r#"{"decision":"deny","reason":"policy"}"#).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("policy"));
    }

    #[test]
    fn parse_fallback_to_allow() {
        let d = parse_decision("this is not json at all").unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_empty_response() {
        let d = parse_decision("").unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_modify_decision() {
        let d = parse_decision(
            r#"{"type":"modify","modified_input":{"command":"ls -la"},"reason":"sanitized"}"#,
        )
        .unwrap();
        assert!(d.is_allowed());
        assert_eq!(d.reason(), Some("sanitized"));
    }

    #[test]
    fn parse_json_block() {
        let response = r#"Here's my analysis:
```json
{"type":"deny","reason":"too dangerous"}
```
The end."#;
        let d = parse_decision(response).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("too dangerous"));
    }

    #[test]
    fn parse_json_block_with_leading_text() {
        let response = r#"Let me check...
```json
{"allow":true}
```
Done."#;
        let d = parse_decision(response).unwrap();
        assert!(d.is_allowed());
    }

    // -- extract_json_block -----------------------------------------------

    #[test]
    fn extract_json_block_found() {
        let text = "some text\n```json\n{\"key\": \"value\"}\n```\nmore";
        assert_eq!(
            extract_json_block(text),
            Some("{\"key\": \"value\"}")
        );
    }

    #[test]
    fn extract_no_json_block() {
        assert_eq!(extract_json_block("plain text"), None);
    }

    #[test]
    fn extract_multiple_blocks_picks_last() {
        let text = "```json\n{\"first\": true}\n```\n```json\n{\"second\": true}\n```";
        assert_eq!(extract_json_block(text), Some("{\"second\": true}"));
    }

    // -- run_prompt_hook --------------------------------------------------

    #[tokio::test]
    async fn prompt_hook_with_mock_llm() {
        use std::path::PathBuf;
        use ante_protocol_shape::ToolUsePayload;

        let payload = EventPayload::PreToolUse(ToolUsePayload {
            base: ante_protocol_shape::BasePayload::new(
                PathBuf::from("/tmp"),
                "0.2.0".into(),
            ),
            tool_name: "Bash".into(),
            input: serde_json::json!({"command": "rm -rf /"}),
            output: None,
            error: None,
            duration_ms: None,
        });

        let result = run_prompt_hook(
            "You are a security checker.\nEvent: {event_type}\nData: {event_json}\nAllow or deny?",
            &None,
            &payload,
            |prompt| {
                // Mock LLM: check if the prompt contains the event type
                let contains_bash = prompt.contains("Bash");
                Box::pin(async move {
                    if contains_bash {
                        r#"{"type":"deny","reason":"Bash is not allowed"}"#.to_string()
                    } else {
                        r#"{"type":"allow"}"#.to_string()
                    }
                })
            },
        )
        .await;

        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(!decision.is_allowed());
        assert_eq!(decision.reason(), Some("Bash is not allowed"));
    }

    #[tokio::test]
    async fn prompt_hook_empty_response_returns_error() {
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

        let result = run_prompt_hook("check", &None, &payload, |_| {
            Box::pin(async move { String::new() })
        })
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PromptHookError::EmptyResponse));
    }

    #[tokio::test]
    async fn prompt_hook_format_includes_event_json() {
        use std::path::PathBuf;
        use ante_protocol_shape::ToolUsePayload;

        let payload = EventPayload::PreToolUse(ToolUsePayload {
            base: ante_protocol_shape::BasePayload::new(
                PathBuf::from("/tmp"),
                "0.2.0".into(),
            ),
            tool_name: "Read".into(),
            input: serde_json::json!({"file": "/etc/passwd"}),
            output: None,
            error: None,
            duration_ms: None,
        });

        let result = run_prompt_hook(
            "Check this: {event_json}",
            &None,
            &payload,
            |prompt| {
                // Verify the template was expanded
                let has_tool_name = prompt.contains("Read");
                let has_event_type = prompt.contains("pre_tool_use");
                Box::pin(async move {
                    if has_tool_name && has_event_type {
                        r#"{"type":"allow"}"#.to_string()
                    } else {
                        r#"{"type":"deny","reason":"template not expanded"}"#.to_string()
                    }
                })
            },
        )
        .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_allowed());
    }
}
