//! MCP-tool hook executor — invokes an MCP server tool with the
//! event payload and parses the response for a HookDecision.
//!
//! This allows hooks to be implemented as MCP tools, enabling hooks
//! to leverage existing MCP ecosystem (e.g., context7 for reference
//! lookup, exa for web-based policy checking).

use std::collections::HashMap;

use ante_protocol_shape::{EventPayload, HookDecision};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum McpToolHookError {
    #[error("MCP tool hook not yet implemented — request passed through")]
    NotImplemented,
}

/// Execute an MCP-tool hook.
///
/// Invokes the specified MCP server tool with the event payload
/// as arguments and parses the response for a HookDecision.
///
/// # Note
/// This is a placeholder. Full MCP tool hooks require the MCP client
/// infrastructure from Phase 4 (US2). For now, all MCP tool hooks
/// return Allow.
pub async fn run_mcp_tool_hook(
    _server: &str,
    _tool: &str,
    _args: &HashMap<String, serde_json::Value>,
    _payload: &EventPayload,
) -> Result<HookDecision, McpToolHookError> {
    // TODO: Implement MCP tool invocation for hooks
    // 1. Get or connect to the MCP server
    // 2. Build tool arguments from event payload
    // 3. Call tools/call via JSON-RPC
    // 4. Parse response for allow/deny/modify
    // 5. Return decision
    //
    // For now, safe default: allow everything.
    Err(McpToolHookError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mcp_tool_hook_not_implemented() {
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

        let result = run_mcp_tool_hook(
            "context7",
            "resolve-library-id",
            &HashMap::new(),
            &payload,
        )
        .await;
        assert!(result.is_err());
    }
}
