//! MCP (Model Context Protocol) client for Ante.
//!
//! Connects to MCP servers via stdio transport, performs the
//! initialize handshake, and calls tools via JSON-RPC.
//!
//! MCP spec: https://spec.modelcontextprotocol.io/

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

/// Errors from MCP client operations.
#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("Failed to spawn MCP server process: {0}")]
    Spawn(std::io::Error),

    #[error("MCP server exited unexpectedly (code {0}): {1}")]
    UnexpectedExit(i32, String),

    #[error("JSON-RPC error: {code} {message}")]
    JsonRpcError { code: i32, message: String },

    #[error("MCP protocol error: {0}")]
    Protocol(String),

    #[error("MCP handshake failed: {0}")]
    Handshake(String),

    #[error("Timeout waiting for MCP response")]
    Timeout,

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(default)]
    data: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerCapabilities {
    pub tools: Option<HashMap<String, Value>>,
    pub resources: Option<HashMap<String, Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInvokeResult {
    pub content: Vec<McpContentItem>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContentItem {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "resource")]
    Resource { resource: McpResourceContent },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub uri: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub mime_type: Option<String>,
}

// ─── MCP Client ─────────────────────────────────────────────────────────────

/// Configuration for reconnection behaviour.
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Whether to auto-reconnect on unexpected disconnection.
    pub enabled: bool,
    /// Max reconnection attempts (0 = unlimited, careful!).
    pub max_attempts: u32,
    /// Base delay for exponential backoff (seconds).
    pub base_delay_secs: u64,
    /// Maximum delay between attempts (seconds).
    pub max_delay_secs: u64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 3,
            base_delay_secs: 1,
            max_delay_secs: 30,
        }
    }
}

/// Operation timeout configuration.
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Timeout for the initialize handshake (seconds).
    pub handshake_secs: u64,
    /// Timeout for tools/list (seconds).
    pub list_tools_secs: u64,
    /// Timeout for tools/call (seconds).
    pub call_tool_secs: u64,
    /// Default I/O read/write timeout (seconds).
    pub io_secs: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            handshake_secs: 30,
            list_tools_secs: 15,
            call_tool_secs: 60,
            io_secs: 10,
        }
    }
}

/// Connected MCP client with transport to a server process.
pub struct McpClient {
    name: String,
    command: String,
    args: Vec<String>,
    child: Option<Child>,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
    capabilities: Option<McpServerCapabilities>,
    connected: Arc<AtomicBool>,
    server_version: Option<String>,
    reconnect: ReconnectConfig,
    timeouts: TimeoutConfig,
}

impl McpClient {
    /// Spawn and handshake with an MCP server.
    pub async fn connect(
        name: &str,
        command: &str,
        args: &[String],
    ) -> Result<Self, McpClientError> {
        Self::connect_with_config(
            name,
            command,
            args,
            ReconnectConfig::default(),
            TimeoutConfig::default(),
        )
        .await
    }

    /// Spawn and handshake with configurable reconnection and timeouts.
    pub async fn connect_with_config(
        name: &str,
        command: &str,
        args: &[String],
        reconnect: ReconnectConfig,
        timeouts: TimeoutConfig,
    ) -> Result<Self, McpClientError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(McpClientError::Spawn)?;

        let stdin = child
            .stdin
            .take()
            .ok_or(McpClientError::Protocol("failed to acquire stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(McpClientError::Protocol("failed to acquire stdout".into()))?;

        let mut client = McpClient {
            name: name.to_string(),
            command: command.to_string(),
            args: args.to_vec(),
            child: Some(child),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            capabilities: None,
            connected: Arc::new(AtomicBool::new(true)),
            server_version: None,
            reconnect,
            timeouts,
        };

        // Perform MCP initialize handshake (with timeout)
        {
            let handshake_timeout = Duration::from_secs(client.timeouts.handshake_secs);
            match timeout(handshake_timeout, client.handshake()).await {
                Ok(Ok((caps, version))) => {
                    client.capabilities = caps;
                    client.server_version = version;
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    client.connected.store(false, Ordering::Relaxed);
                    return Err(McpClientError::Timeout);
                }
            }
        }

        // Refresh tools
        client.refresh_tools().await?;

        Ok(client)
    }

    /// MCP initialize handshake.
    async fn handshake(
        &mut self,
    ) -> Result<(Option<McpServerCapabilities>, Option<String>), McpClientError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {
                    "tools": {}
                },
                "clientInfo": {
                    "name": "ante-agent",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        self.send_raw(&request.to_string()).await?;
        let response = self.read_line().await?;
        let parsed: JsonRpcResponse = serde_json::from_str(&response)?;

        if let Some(err) = parsed.error {
            return Err(McpClientError::Handshake(format!(
                "initialize failed: {} (code {})",
                err.message, err.code
            )));
        }

        let result = parsed.result.ok_or(McpClientError::Handshake(
            "no result in initialize response".into(),
        ))?;

        let capabilities = result
            .get("capabilities")
            .map(|c| serde_json::from_value(c.clone()))
            .transpose()?;

        let server_version = result
            .get("serverInfo")
            .and_then(|i| i.get("version"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Send initialized notification (no response expected)
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        self.send_raw(&notif.to_string()).await?;

        Ok((capabilities, server_version))
    }

    /// Refresh tool list from the server.
    pub async fn refresh_tools(&mut self) -> Result<Vec<McpToolDefinition>, McpClientError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list"
        });

        self.send_raw(&request.to_string()).await?;
        let response = self.read_line().await?;
        let parsed: JsonRpcResponse = serde_json::from_str(&response)?;

        if let Some(err) = parsed.error {
            return Err(McpClientError::JsonRpcError {
                code: err.code,
                message: err.message,
            });
        }

        let tools = parsed
            .result
            .and_then(|r| r.get("tools").cloned())
            .map(|t| serde_json::from_value::<Vec<McpToolDefinition>>(t))
            .transpose()?
            .unwrap_or_default();

        Ok(tools)
    }

    /// Invoke an MCP tool and return the result.
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpInvokeResult, McpClientError> {
        if !self.connected.load(Ordering::Relaxed) {
            return Err(McpClientError::ConnectionClosed);
        }

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        let call_timeout = Duration::from_secs(self.timeouts.call_tool_secs);

        match timeout(call_timeout, async {
            let mut stdin = self.stdin.lock().await;
            let req_str = request.to_string();
            stdin.write_all(req_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
            drop(stdin);

            let response = self.read_line().await?;
            Ok::<_, McpClientError>(response)
        })
        .await
        {
            Ok(Ok(response)) => {
                let parsed: JsonRpcResponse = serde_json::from_str(&response)?;

                if let Some(err) = parsed.error {
                    return Err(McpClientError::JsonRpcError {
                        code: err.code,
                        message: err.message,
                    });
                }

                let result: McpInvokeResult = parsed
                    .result
                    .map(|r| serde_json::from_value(r))
                    .transpose()?
                    .ok_or(McpClientError::Protocol(
                        "no result in tools/call response".into(),
                    ))?;

                Ok(result)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.connected.store(false, Ordering::Relaxed);
                Err(McpClientError::Timeout)
            }
        }
    }

    /// Human-readable description of the connection.
    pub fn info(&self) -> String {
        format!(
            "mcp:{} (v{})",
            self.name,
            self.server_version.as_deref().unwrap_or("?")
        )
    }

    /// The server's capabilities.
    pub fn capabilities(&self) -> Option<&McpServerCapabilities> {
        self.capabilities.as_ref()
    }

    /// Whether the client is connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Gracefully shut down the client.
    pub async fn shutdown(&mut self) {
        self.connected.store(false, Ordering::Relaxed);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    /// Reconnect to the MCP server (kill + respawn + handshake).
    ///
    /// Returns the old server version string if available.
    pub async fn reconnect(&mut self) -> Result<Option<String>, McpClientError> {
        let old_version = self.server_version.clone();

        // Clean up the old process
        self.connected.store(false, Ordering::Relaxed);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }

        // Spawn a new process
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(McpClientError::Spawn)?;

        let stdin = child.stdin.take().ok_or(McpClientError::Protocol(
            "failed to acquire stdin on reconnect".into(),
        ))?;
        let stdout = child.stdout.take().ok_or(McpClientError::Protocol(
            "failed to acquire stdout on reconnect".into(),
        ))?;

        // Replace internals
        self.child = Some(child);
        *self.stdin.lock().await = stdin;
        *self.stdout.lock().await = BufReader::new(stdout);
        self.connected.store(true, Ordering::Relaxed);
        self.next_id.store(1, Ordering::Relaxed);

        // Re-do handshake
        let handshake_timeout = Duration::from_secs(self.timeouts.handshake_secs);
        match timeout(handshake_timeout, self.handshake()).await {
            Ok(Ok((caps, version))) => {
                self.capabilities = caps;
                self.server_version = version;
                Ok(old_version)
            }
            Ok(Err(e)) => {
                self.connected.store(false, Ordering::Relaxed);
                Err(e)
            }
            Err(_) => {
                self.connected.store(false, Ordering::Relaxed);
                Err(McpClientError::Timeout)
            }
        }
    }

    /// Attempt reconnection with exponential backoff.
    ///
    /// Runs up to `max_attempts` retries with base_delay_secs doubling each time.
    /// Returns `Ok(())` on success or the last error after exhausting retries.
    pub async fn try_reconnect(&mut self) -> Result<(), McpClientError> {
        if !self.reconnect.enabled || self.reconnect.max_attempts == 0 {
            return self.reconnect().await.map(|_| ());
        }

        let mut last_err = McpClientError::ConnectionClosed;

        for attempt in 1..=self.reconnect.max_attempts {
            let delay = Duration::from_secs(
                (self.reconnect.base_delay_secs << (attempt.saturating_sub(1)))
                    .min(self.reconnect.max_delay_secs),
            );
            sleep(delay).await;

            match self.reconnect().await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }

    // ── Internal helpers ──

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn send_raw(&self, data: &str) -> Result<(), McpClientError> {
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(data.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }

    async fn read_line(&self) -> Result<String, McpClientError> {
        let mut stdout = self.stdout.lock().await;
        let mut line = String::new();
        stdout.read_line(&mut line).await?;
        if line.is_empty() {
            self.connected.store(false, Ordering::Relaxed);
            return Err(McpClientError::ConnectionClosed);
        }
        Ok(line.trim().to_string())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::Relaxed);
        if let Some(mut child) = self.child.take() {
            // Detach — let the runtime handle it
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal MCP server for testing: reads a JSON-RPC request, returns
    /// a hardcoded initialize response, then tools/list response, then
    /// any tools/call response.
    #[tokio::test]
    async fn mcp_client_connects_and_discovers_tools() {
        // Spawn a test MCP server subprocess (using cat to echo back)
        // Actually, use a Python-based test server since we can spawn it inline
        let server_script = r#"
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    if req["method"] == "initialize":
        resp = {
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "test-server", "version": "1.0.0"}
            }
        }
    elif req["method"] == "notifications/initialized":
        continue
    elif req["method"] == "tools/list":
        resp = {
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {
                "tools": [{"name": "echo", "description": "Echo", "inputSchema": {"type": "object"}}]
            }
        }
    elif req["method"] == "tools/call":
        resp = {
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": {
                "content": [{"type": "text", "text": "ok"}],
                "is_error": False
            }
        }
    else:
        continue
    sys.stdout.write(json.dumps(resp) + "\n")
    sys.stdout.flush()
"#;

        // Write server script to temp file
        let tmp = tempfile::tempdir().expect("temp dir");
        let script_path = tmp.path().join("mcp_test_server.py");
        std::fs::write(&script_path, server_script).expect("write script");

        let mut client =
            McpClient::connect("test", "python3", &[script_path.display().to_string()])
                .await
                .expect("connect");

        assert!(client.is_connected());
        assert!(client.server_version.is_some());
        assert_eq!(client.server_version.as_deref(), Some("1.0.0"));

        // Refresh tools and verify discovery
        let tools = client.refresh_tools().await.expect("list tools");
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");

        // Call tool
        let result = client
            .call_tool("echo", serde_json::json!({"text": "hello"}))
            .await
            .expect("call tool");
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);

        client.shutdown().await;
    }
}
