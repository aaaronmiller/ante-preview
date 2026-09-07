//! Embedded memory MCP server for Ante.
//!
//! Exposes memory operations as MCP tools. In Phase 1, this wraps
//! the MemoryStore in a stdio-based MCP server. Full MCP protocol
//! compliance will be added in Phase 2.

use std::path::PathBuf;

use crate::memory::store::MemoryStore;

use super::store::MemoryEntry;

/// Embedded memory server that provides MCP-compatible access
/// to the memory store.
pub struct MemoryServer {
    store: MemoryStore,
}

impl MemoryServer {
    /// Open the memory server with the given db path.
    pub fn open(db_path: PathBuf, max_context: usize) -> Result<Self, crate::memory::store::MemoryError> {
        let store = MemoryStore::open(db_path, max_context)?;
        Ok(MemoryServer { store })
    }

    /// Handle a `memory_add` tool call.
    pub fn add_memory(&mut self, content: String, tags: String, project: String) -> Result<MemoryEntry, String> {
        self.store.add(content, tags, project).map_err(|e| e.to_string())
    }

    /// Handle a `memory_search` tool call.
    pub fn search(&self, query: &str) -> Vec<serde_json::Value> {
        self.store.search(query)
            .into_iter()
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect()
    }

    /// Handle a `memory_get_context` tool call.
    pub fn get_context(&self, project: &str, max_results: usize) -> Vec<serde_json::Value> {
        self.store.get_context(project, max_results)
            .into_iter()
            .map(|e| serde_json::to_value(e).unwrap_or_default())
            .collect()
    }

    /// Number of stored memories.
    pub fn count(&self) -> usize {
        self.store.count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_search_via_server() {
        let tmp = tempfile::tempdir().unwrap();
        let mut server = MemoryServer::open(tmp.path().join("mem.json"), 20).unwrap();

        server.add_memory("API key is secret123".into(), "credentials".into(), "myapp".into()).unwrap();

        let results = server.search("secret123");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn get_context_returns_relevant() {
        let tmp = tempfile::tempdir().unwrap();
        let mut server = MemoryServer::open(tmp.path().join("m.json"), 10).unwrap();

        server.add_memory("config uses port 3000".into(), "config".into(), "webapp".into()).unwrap();
        server.add_memory("database url is localhost".into(), "db".into(), "webapp".into()).unwrap();

        let ctx = server.get_context("webapp", 10);
        assert_eq!(ctx.len(), 2);
    }

    #[test]
    fn empty_server_returns_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let server = MemoryServer::open(tmp.path().join("m.json"), 20).unwrap();
        assert_eq!(server.count(), 0);
    }
}
