//! MCP tool registry — discovers tools from all connected MCP servers
//! and provides lookup() / invoke() under the `mcp__` namespace.

use serde_json::Value;
use thiserror::Error;

use super::client::{McpClient, McpClientError, McpInvokeResult, McpToolDefinition};

/// Namespaced MCP tool identifier: `mcp__{server}__{tool}`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct McpToolId {
    /// Server name (from settings config).
    pub server: String,
    /// Tool name within the server.
    pub tool: String,
}

impl std::fmt::Display for McpToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mcp__{}__{}", self.server, self.tool)
    }
}

impl McpToolId {
    /// Parse a `mcp__server__tool` string.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(3, "__").collect();
        if parts.len() == 3 && parts[0] == "mcp" {
            Some(McpToolId {
                server: parts[1].to_string(),
                tool: parts[2].to_string(),
            })
        } else {
            None
        }
    }
}

/// Errors from tool registry operations.
#[derive(Debug, Error)]
pub enum McpRegistryError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Server not connected: {0}")]
    ServerNotConnected(String),

    #[error("MCP client error: {0}")]
    Client(#[from] McpClientError),

    #[error("Invoke failed: {0}")]
    Invoke(String),
}

/// Registered MCP server with its connected client.
pub struct McpServerHandle {
    pub name: String,
    pub client: McpClient,
    pub tools: Vec<McpToolDefinition>,
    pub config: McpServerConfigEntry,
}

/// Stored configuration entry for an MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfigEntry {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub auto_start: bool,
}

/// Tool registry that manages MCP server connections and provides
/// namespace tool invocation.
pub struct McpToolRegistry {
    servers: Vec<McpServerHandle>,
}

impl McpToolRegistry {
    pub fn new() -> Self {
        McpToolRegistry {
            servers: Vec::new(),
        }
    }

    /// Connect to an MCP server and register its tools.
    pub async fn register_server(
        &mut self,
        config: McpServerConfigEntry,
    ) -> Result<(), McpRegistryError> {
        let mut client = McpClient::connect(&config.name, &config.command, &config.args).await?;
        let tools = client.refresh_tools().await?;

        let handle = McpServerHandle {
            name: config.name.clone(),
            client,
            tools,
            config,
        };

        self.servers.push(handle);
        Ok(())
    }

    /// Look up a tool ID and return its definition + server handle index.
    pub fn lookup_tool(&self, tool_id: &McpToolId) -> Option<(usize, &McpToolDefinition)> {
        for (idx, server) in self.servers.iter().enumerate() {
            if server.name == tool_id.server {
                for tool in &server.tools {
                    if tool.name == tool_id.tool {
                        return Some((idx, tool));
                    }
                }
            }
        }
        None
    }

    /// Invoke a tool by its McpToolId.
    pub async fn invoke(
        &self,
        tool_id: &McpToolId,
        arguments: Value,
    ) -> Result<McpInvokeResult, McpRegistryError> {
        let (idx, _) = self
            .lookup_tool(tool_id)
            .ok_or_else(|| McpRegistryError::ToolNotFound(tool_id.to_string()))?;

        let server = &self.servers[idx];
        if !server.client.is_connected() {
            return Err(McpRegistryError::ServerNotConnected(server.name.clone()));
        }

        let result = server.client.call_tool(&tool_id.tool, arguments).await?;
        Ok(result)
    }

    /// List all registered tool IDs.
    pub fn list_tools(&self) -> Vec<McpToolId> {
        let mut ids = Vec::new();
        for server in &self.servers {
            for tool in &server.tools {
                ids.push(McpToolId {
                    server: server.name.clone(),
                    tool: tool.name.clone(),
                });
            }
        }
        ids
    }

    /// Get a reference to a connected server handle.
    pub fn server(&self, name: &str) -> Option<&McpServerHandle> {
        self.servers.iter().find(|s| s.name == name)
    }

    /// Disconnect a server.
    pub async fn disconnect(&mut self, name: &str) {
        if let Some(idx) = self.servers.iter().position(|s| s.name == name) {
            let mut server = self.servers.remove(idx);
            server.client.shutdown().await;
        }
    }

    /// Number of connected servers.
    pub fn server_count(&self) -> usize {
        self.servers.len()
    }
}

impl Default for McpToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mcp_tool_id() {
        let id = McpToolId::parse("mcp__filesystem__list_directory");
        assert!(id.is_some());
        assert_eq!(id.as_ref().unwrap().server, "filesystem");
        assert_eq!(id.as_ref().unwrap().tool, "list_directory");
    }

    #[test]
    fn parse_mcp_tool_id_rejects_invalid() {
        assert!(McpToolId::parse("invalid").is_none());
        assert!(McpToolId::parse("mcp__onlyone").is_none());
        assert!(McpToolId::parse("").is_none());
    }

    #[test]
    fn mcp_tool_id_display() {
        let id = McpToolId {
            server: "my-srv".into(),
            tool: "my_tool".into(),
        };
        assert_eq!(id.to_string(), "mcp__my-srv__my_tool");
    }

    #[test]
    fn empty_registry_lists_nothing() {
        let registry = McpToolRegistry::new();
        assert_eq!(registry.server_count(), 0);
        assert!(registry.list_tools().is_empty());
    }
}
