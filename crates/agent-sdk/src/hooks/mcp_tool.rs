//! MCP-tool hook executor — invokes an MCP server tool with the
//! event payload and parses the response for a HookDecision.
//!
//! This allows hooks to be implemented as MCP tools, enabling hooks
//! to leverage existing MCP ecosystem (e.g., context7 for reference
//! lookup, exa for web-based policy checking).
//!
//! The actual MCP invocation is provided via the `InvokeMcp` callback
//! defined in `super::mod` so that the MCP client lifecycle is managed
//! by the caller (e.g. the `ante` crate's `McpToolRegistry`).

use std::collections::HashMap;

use ante_protocol_shape::{EventPayload, HookDecision};
use serde_json::Value;
use thiserror::Error;

use super::InvokeMcp;

#[derive(Debug, Error)]
pub enum McpToolHookError {
    #[error("MCP invocation returned no content")]
    EmptyResponse,

    #[error("MCP invocation failed: {0}")]
    InvokeFailed(String),

    #[error("Failed to parse HookDecision from MCP response: {detail} (response: {response})")]
    ParseDecision { detail: String, response: String },
}

/// Execute an MCP-tool hook.
///
/// Invokes the specified MCP server tool with the event payload
/// as arguments and parses the response for a HookDecision.
///
/// # Callback
///
/// `invoke_mcp` receives (server_name, tool_name, arguments) and must
/// return the tool's JSON response on success or an error string on
/// failure.
pub async fn run_mcp_tool_hook(
    invoke_mcp: &InvokeMcp,
    server: &str,
    tool: &str,
    args: &HashMap<String, Value>,
    payload: &EventPayload,
) -> Result<HookDecision, McpToolHookError> {
    // ── 1. Build arguments ─────────────────────────────────────────────
    let mut call_args = args.clone();

    // Inject the event payload under an "event" key so the MCP tool
    // receives full context about what triggered the hook.
    let event_json = serde_json::to_value(payload)
        .unwrap_or(Value::Null);
    call_args.insert("event".to_string(), event_json);

    // Also inject convenient top-level fields from common payload types
    // so MCP tools can easily check tool_name, event_type, etc.
    {
        let event_type_s = payload.event_type().to_string();
        call_args.insert("event_type".to_string(), Value::String(event_type_s));

        // Extract tool_name if this is a tool event
        match payload {
            EventPayload::PreToolUse(p)
            | EventPayload::PostToolUse(p)
            | EventPayload::PostToolUseFailure(p) => {
                call_args.insert("tool_name".to_string(), Value::String(p.tool_name.clone()));
            }
            _ => {}
        }
    }

    // ── 2. Invoke MCP tool ────────────────────────────────────────────
    let result = invoke_mcp(server.to_string(), tool.to_string(), Value::Object(
        call_args.into_iter().map(|(k, v)| (k, v)).collect(),
    ))
    .await
    .map_err(|e| McpToolHookError::InvokeFailed(e))?;

    // ── 3. Parse response ─────────────────────────────────────────────
    parse_mcp_response(&result)
}

/// Parse a `HookDecision` from the MCP tool's JSON response.
///
/// The response can be:
/// - A direct `HookDecision` JSON object with `type: "allow"|"deny"|"modify"`
/// - An object with `content` array (standard MCP response envelope)
/// - An object with `allow`/`decision` boolean/string fields
pub fn parse_mcp_response(value: &Value) -> Result<HookDecision, McpToolHookError> {
    // If it's a direct HookDecision with a "type" field, deserialize it
    if let Some(ty) = value.get("type").and_then(|v| v.as_str()) {
        match ty {
            "allow" => return Ok(HookDecision::Allow),
            "deny" => {
                let reason = value.get("reason").and_then(|v| v.as_str()).map(String::from);
                return Ok(HookDecision::Deny { reason });
            }
            "modify" => {
                let mi = value
                    .get("modifiedInput")
                    .or_else(|| value.get("modified_input"))
                    .or_else(|| value.get("modifiedInput"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let reason = value.get("reason").and_then(|v| v.as_str()).map(String::from);
                return Ok(HookDecision::Modify {
                    modified_input: mi,
                    reason,
                });
            }
            _ => {}
        }
    }

    // Standard MCP response envelope: { "content": [{ "type": "text", "text": "..." }] }
    if let Some(content) = value.get("content").and_then(|v| v.as_array()) {
        for item in content {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                // Try parsing the text content as JSON HookDecision
                if let Ok(d) = serde_json::from_str::<HookDecision>(text) {
                    return Ok(d);
                }
                // Try parsing as plain JSON
                if let Ok(v) = serde_json::from_str::<Value>(text) {
                    return parse_mcp_response(&v);
                }
                // Check for specific text patterns
                let lower = text.to_lowercase();
                if lower.contains("deni") || lower.contains("reject") || lower.contains("block") {
                    return Ok(HookDecision::Deny {
                        reason: Some(text.to_string()),
                    });
                }
                if lower.contains("allow") || lower.contains("approve") {
                    return Ok(HookDecision::Allow);
                }
            }
        }
    }

    // Fallback: try { "allow": true/false }
    if let Some(allowed) = value.get("allow").and_then(|v| v.as_bool()) {
        if allowed {
            return Ok(HookDecision::Allow);
        } else {
            let reason = value.get("reason").and_then(|v| v.as_str()).map(String::from);
            return Ok(HookDecision::Deny { reason });
        }
    }

    // Last resort: allow by default
    Ok(HookDecision::Allow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;

    // -- parse_mcp_response ----------------------------------------------

    #[test]
    fn parse_direct_allow() {
        let v = json!({"type":"allow"});
        let d = parse_mcp_response(&v).unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_direct_deny() {
        let v = json!({"type":"deny","reason":"blocked"});
        let d = parse_mcp_response(&v).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("blocked"));
    }

    #[test]
    fn parse_direct_modify() {
        let v = json!({"type":"modify","modified_input":{"command":"ls -la"},"reason":"sanitized"});
        let d = parse_mcp_response(&v).unwrap();
        assert!(d.is_allowed());
        assert_eq!(d.reason(), Some("sanitized"));
    }

    #[test]
    fn parse_mcp_content_envelope() {
        let v = json!({
            "content": [{
                "type": "text",
                "text": "{\"type\":\"deny\",\"reason\":\"policy violation\"}"
            }]
        });
        let d = parse_mcp_response(&v).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("policy violation"));
    }

    #[test]
    fn parse_mcp_content_fallback_text() {
        let v = json!({
            "content": [{
                "type": "text",
                "text": "This operation should be denied because it modifies system files."
            }]
        });
        let d = parse_mcp_response(&v).unwrap();
        assert!(!d.is_allowed());
    }

    #[test]
    fn parse_mcp_content_allow_text() {
        let v = json!({
            "content": [{
                "type": "text",
                "text": "Approved. The operation is safe."
            }]
        });
        let d = parse_mcp_response(&v).unwrap();
        assert!(d.is_allowed());
    }

    #[test]
    fn parse_allow_boolean_fallback() {
        let v = json!({"allow": false, "reason": "blacklisted"});
        let d = parse_mcp_response(&v).unwrap();
        assert!(!d.is_allowed());
        assert_eq!(d.reason(), Some("blacklisted"));
    }

    #[test]
    fn parse_empty_fallback_to_allow() {
        let v = json!({});
        let d = parse_mcp_response(&v).unwrap();
        assert!(d.is_allowed());
    }

    // -- run_mcp_tool_hook -----------------------------------------------

    #[tokio::test]
    async fn mcp_tool_hook_with_mock_server() {
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

        let server_name = "policy-checker".to_string();
        let tool_name = "check-tool".to_string();

        // Create a mock InvokeMcp callback
        let invoke_mcp: InvokeMcp = Arc::new(
            move |srv: String, tool: String, args: Value| -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>> {
                assert_eq!(srv, "policy-checker");
                assert_eq!(tool, "check-tool");
                let has_event = args.get("event").is_some();
                let has_tool_name = args.get("tool_name")
                    .and_then(|v| v.as_str())
                    == Some("Bash");
                Box::pin(async move {
                    if has_event && has_tool_name {
                        Ok(json!({"type":"deny","reason":"dangerous tool"}))
                    } else {
                        Ok(json!({"type":"allow"}))
                    }
                })
            },
        );

        let result = run_mcp_tool_hook(
            &invoke_mcp,
            &server_name,
            &tool_name,
            &HashMap::new(),
            &payload,
        )
        .await;

        assert!(result.is_ok());
        let decision = result.unwrap();
        assert!(!decision.is_allowed());
        assert_eq!(decision.reason(), Some("dangerous tool"));
    }

    #[tokio::test]
    async fn mcp_tool_hook_invoke_error_defaults_allow() {
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

        let invoke_mcp: InvokeMcp = Arc::new(
            |_srv: String, _tool: String, _args: Value| -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>> {
                Box::pin(async move { Err("server not available".to_string()) })
            },
        );

        let result = run_mcp_tool_hook(
            &invoke_mcp,
            "broken",
            "fail",
            &HashMap::new(),
            &payload,
        )
        .await;

        // Error on invocation should propagate up
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            McpToolHookError::InvokeFailed(_)
        ));
    }
}
