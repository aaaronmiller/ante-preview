//! Agent Client Protocol (ACP) adapter for Ante.
//!
//! JSON-RPC 2.0 over stdio, following the ACP v1 lifecycle: `initialize`
//! negotiates the protocol version, `session/new` opens a conversation backed
//! by [`SessionManager`], and `session/prompt` runs one turn through a
//! [`Backend`], streaming the answer as a `session/update`
//! `agent_message_chunk` notification before replying `{stopReason}`.
//! `session/cancel` interrupts an in-flight turn when the backend supports it.

pub mod agent;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol version this adapter speaks.
pub const PROTOCOL_VERSION: u32 = 1;

/// JSON-RPC 2.0 request id (number or string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    Text(String),
}

/// One incoming JSON-RPC line: a request (with `id`) or a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcIncoming {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<RpcId>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// Outgoing JSON-RPC response. `id` is null only for unparseable input.
#[derive(Debug, Clone, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<RpcId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcResponse {
    pub fn ok(id: Option<RpcId>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<RpcId>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }

    pub fn render(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"encode failure"}}"#
                .into()
        })
    }
}

/// Outgoing JSON-RPC notification (no `id`).
#[derive(Debug, Clone, Serialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl RpcNotification {
    pub fn render(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// Error codes: standard JSON-RPC range plus server-defined session errors.
pub mod codes {
    pub const PARSE: i64 = -32700;
    #[allow(dead_code)]
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const BACKEND: i64 = -32000;
    pub const UNKNOWN_SESSION: i64 = -32001;
    pub const NOT_INITIALIZED: i64 = -32002;
}

/// `initialize` params (only the version is negotiated here).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(default, rename = "protocolVersion")]
    pub protocol_version: Option<u32>,
}

/// `session/new` params.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewSessionParams {
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

/// One prompt content block. Non-text blocks are accepted and skipped.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentBlock {
    #[serde(default, rename = "type")]
    pub block_type: String,
    #[serde(default)]
    pub text: Option<String>,
}

/// `session/prompt` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    #[serde(default)]
    pub prompt: Vec<ContentBlock>,
}

/// `session/cancel` params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelParams {
    #[serde(rename = "sessionId")]
    pub session_id: String,
}

/// Collect the text blocks of a prompt, in order.
pub fn prompt_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| b.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a `session/update` chunk notification for one answer text.
pub fn chunk_notification(session_id: &str, text: &str) -> RpcNotification {
    RpcNotification {
        jsonrpc: "2.0".into(),
        method: "session/update".into(),
        params: serde_json::json!({
            "sessionId": session_id,
            "update": { "type": "agent_message_chunk", "text": text },
        }),
    }
}
