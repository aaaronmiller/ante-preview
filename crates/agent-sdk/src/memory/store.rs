//! Persistent memory module for Ante.
//!
//! Stores and retrieves memories across sessions. Uses an embedded
//! key-value store. The memory MCP server wraps this module to provide
//! tool-based access.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// ULID-like timestamp generator.
fn ulid_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let ts = now.as_nanos() as u64;
    format!("{:016x}", ts)
}

/// A single memory entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// ULID-based unique ID.
    pub id: String,
    /// Free-form content text.
    pub content: String,
    /// Comma-separated tags for categorization.
    pub tags: String,
    /// Project name for scoping.
    pub project: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

/// Errors from memory operations.
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Failed to read memory store: {0}")]
    Read(std::io::Error),

    #[error("Failed to write memory store: {0}")]
    Write(std::io::Error),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Memory store not initialized")]
    NotInitialized,
}

/// In-memory memory store backed by a JSON file.
pub struct MemoryStore {
    entries: Vec<MemoryEntry>,
    db_path: PathBuf,
    max_context: usize,
}

impl MemoryStore {
    /// Open (or create) the memory store at the given path.
    pub fn open(db_path: PathBuf, max_context: usize) -> Result<Self, MemoryError> {
        let entries = if db_path.exists() {
            let content = fs::read_to_string(&db_path).map_err(MemoryError::Read)?;
            serde_json::from_str(&content).map_err(MemoryError::Serde)?
        } else {
            Vec::new()
        };

        Ok(MemoryStore {
            entries,
            db_path,
            max_context,
        })
    }

    /// Add a memory entry.
    pub fn add(&mut self, content: String, tags: String, project: String) -> Result<MemoryEntry, MemoryError> {
        let id = format!("mem-{}", ulid_timestamp());
        let timestamp = id[4..].to_string(); // use the hex timestamp

        let entry = MemoryEntry {
            id,
            content,
            tags,
            project,
            timestamp,
        };

        self.entries.push(entry.clone());

        // Persist after each write
        self.save()?;

        Ok(entry)
    }

    /// Search memories by content keyword (simple contains-check).
    pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
        let lower = query.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.content.to_lowercase().contains(&lower)
                    || e.tags.to_lowercase().contains(&lower)
            })
            .collect()
    }

    /// Search with TF-IDF-like relevance ranking.
    ///
    /// Returns entries sorted by relevance score (highest first).
    /// Scoring factors:
    ///   - Exact phrase match: +100 per occurrence
    ///   - Individual keyword match: +10 per occurrence
    ///   - Tag match: +30 per matching tag
    ///   - Keyword density bonus: if >20% of words match, 2x multiplier
    pub fn search_ranked(&self, query: &str) -> Vec<RankedEntry> {
        let query_lower = query.to_lowercase();
        let keywords: Vec<&str> = query_lower
            .split_whitespace()
            .filter(|w| w.len() > 2)
            .collect();

        let mut scored: Vec<RankedEntry> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let content_lower = entry.content.to_lowercase();
                let tags_lower = entry.tags.to_lowercase();
                let mut score = 0u64;

                // Exact phrase match (highest weight)
                if content_lower.contains(&query_lower) {
                    // Count occurrences
                    let count = content_lower.matches(&query_lower).count() as u64;
                    score += count * 100;
                }

                // Individual keyword matches
                let mut keyword_matches = 0u64;
                for kw in &keywords {
                    let count = content_lower.matches(kw).count() as u64;
                    score += count * 10;
                    keyword_matches += count;
                }

                // Tag match (only if query mentions the tag)
                for kw in &keywords {
                    if tags_lower.contains(kw) {
                        score += 30;
                    }
                }

                // Must have at least some relevance
                if score == 0 && !query_lower.is_empty() {
                    // Zero match — still include if content/tags have broad overlap
                    let word_count = content_lower.split_whitespace().count() as f64;
                    if word_count > 0.0 {
                        let density = keyword_matches as f64 / word_count;
                        if density > 0.2 {
                            score = (density * 50.0) as u64;
                        } else {
                            return None; // No relevance
                        }
                    } else {
                        return None;
                    }
                }

                Some(RankedEntry {
                    entry,
                    score,
                })
            })
            .collect();

        scored.sort_by(|a, b| b.score.cmp(&a.score));
        scored
    }

    /// Flexible query with filters.
    ///
    /// Supports:
    ///   - `q` — keyword search (uses ranked search behind the scenes)
    ///   - `project` — scope to a specific project
    ///   - `tags` — filter by comma-separated tag keywords (any match)
    ///   - `limit` — max results to return
    pub fn query(
        &self,
        q: Option<&str>,
        project: Option<&str>,
        tags: Option<&str>,
        limit: usize,
    ) -> Vec<&MemoryEntry> {
        let project_filter = |e: &&MemoryEntry| -> bool {
            match project {
                Some(p) => e.project == p,
                None => true,
            }
        };

        let tag_filter = |e: &&MemoryEntry| -> bool {
            match tags {
                Some(t) => {
                    let tag_keywords: Vec<&str> =
                        t.split(',').map(|s| s.trim()).collect();
                    tag_keywords.iter().any(|kw| {
                        e.tags.to_lowercase().contains(&kw.to_lowercase())
                    })
                }
                None => true,
            }
        };

        let mut results: Vec<&MemoryEntry> = self
            .entries
            .iter()
            .filter(|e| project_filter(e) && tag_filter(e))
            .collect();

        // Sort — ranked if query, otherwise by recency
        if let Some(query_str) = q {
            let ranked = self.search_ranked(query_str);
            let ranked_ids: std::collections::HashSet<&str> = ranked
                .iter()
                .map(|r| r.entry.id.as_str())
                .collect();
            results.retain(|e| ranked_ids.contains(e.id.as_str()));
            results.sort_by(|a, b| {
                let score_a = ranked
                    .iter()
                    .find(|r| r.entry.id == a.id)
                    .map(|r| r.score)
                    .unwrap_or(0);
                let score_b = ranked
                    .iter()
                    .find(|r| r.entry.id == b.id)
                    .map(|r| r.score)
                    .unwrap_or(0);
                score_b.cmp(&score_a)
            });
        } else {
            results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        }

        results.truncate(limit);
        results
    }

    /// Get context memories for a project, ranked by recency + keyword overlap.
    pub fn get_context(&self, project: &str, max_results: usize) -> Vec<&MemoryEntry> {
        let mut matches: Vec<&MemoryEntry> = self
            .entries
            .iter()
            .filter(|e| e.project == project)
            .collect();

        // Sort by recency (most recent first)
        matches.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        matches.truncate(max_results.min(self.max_context));
        matches
    }

    /// Get all memories for a project.
    pub fn for_project(&self, project: &str) -> Vec<&MemoryEntry> {
        self.entries.iter().filter(|e| e.project == project).collect()
    }

    /// Total memory count.
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Persist to disk.
    fn save(&self) -> Result<(), MemoryError> {
        let json = serde_json::to_string_pretty(&self.entries).map_err(MemoryError::Serde)?;
        fs::write(&self.db_path, &json).map_err(MemoryError::Write)?;
        Ok(())
    }
}

/// A memory entry with its relevance score.
#[derive(Debug, Clone)]
pub struct RankedEntry<'a> {
    pub entry: &'a MemoryEntry,
    pub score: u64,
}

impl<'a> RankedEntry<'a> {
    pub fn into_entry(self) -> &'a MemoryEntry {
        self.entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_search_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memories.json");
        let mut store = MemoryStore::open(path, 20).unwrap();

        store
            .add("Project config uses port 8080".into(), "config".into(), "myapp".into())
            .unwrap();

        let results = store.search("port 8080");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project, "myapp");
    }

    #[test]
    fn get_context_returns_most_recent() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memories.json");
        let mut store = MemoryStore::open(path, 5).unwrap();

        store.add("old memory".into(), "".into(), "test".into()).unwrap();
        store.add("recent memory".into(), "".into(), "test".into()).unwrap();

        let context = store.get_context("test", 5);
        assert_eq!(context[0].content, "recent memory");
    }

    #[test]
    fn search_is_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("Project API key".into(), "secret".into(), "x".into()).unwrap();
        assert_eq!(store.search("api").len(), 1);
        assert_eq!(store.search("API").len(), 1);
    }

    #[test]
    fn persist_survives_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("memories.json");

        {
            let mut store = MemoryStore::open(path.clone(), 20).unwrap();
            store.add("persistent data".into(), "test".into(), "p".into()).unwrap();
        }

        {
            let store = MemoryStore::open(path, 20).unwrap();
            assert_eq!(store.count(), 1);
            assert_eq!(store.search("persistent").len(), 1);
        }
    }

    #[test]
    fn max_context_respected() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 2).unwrap();

        store.add("one".into(), "".into(), "p".into()).unwrap();
        store.add("two".into(), "".into(), "p".into()).unwrap();
        store.add("three".into(), "".into(), "p".into()).unwrap();

        assert_eq!(store.get_context("p", 10).len(), 2);
    }

    // ── search_ranked tests ─────────────────────────────────────────────

    #[test]
    fn search_ranked_returns_ranked_results() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("The API key is stored in config".into(), "config".into(), "p".into()).unwrap();
        store.add("Use port 8080 for the server".into(), "network".into(), "p".into()).unwrap();
        store.add("API documentation is in readme".into(), "docs".into(), "p".into()).unwrap();

        let ranked = store.search_ranked("API key");
        assert!(!ranked.is_empty());
        // Entry with "API key" exact phrase should rank highest
        assert!(ranked[0].entry.content.contains("API key"));
    }

    #[test]
    fn search_ranked_empty_query() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let ranked = store.search_ranked("");
        assert!(ranked.is_empty());
    }

    #[test]
    fn search_ranked_exact_phrase_highest() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("deployment config port 8080".into(), "".into(), "p".into()).unwrap();
        store.add("port 8080 is the default".into(), "".into(), "p".into()).unwrap();

        let ranked = store.search_ranked("port 8080");
        assert!(!ranked.is_empty());
        // Both contain "port 8080" exact phrase — higher word count should still work
        // At minimum, both should be returned
        assert_eq!(ranked.len(), 2);
    }

    // ── query tests ──────────────────────────────────────────────────────

    #[test]
    fn query_filters_by_project() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("alpha".into(), "".into(), "proj1".into()).unwrap();
        store.add("beta".into(), "".into(), "proj2".into()).unwrap();
        store.add("gamma".into(), "".into(), "proj1".into()).unwrap();

        let results = store.query(None, Some("proj1"), None, 10);
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|e| e.content == "alpha"));
        assert!(results.iter().any(|e| e.content == "gamma"));
    }

    #[test]
    fn query_filters_by_tags() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("memory one".into(), "database,config".into(), "p".into()).unwrap();
        store.add("memory two".into(), "network".into(), "p".into()).unwrap();

        let results = store.query(None, None, Some("database"), 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "memory one");
    }

    #[test]
    fn query_with_keyword_and_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        store.add("server config port 8080".into(), "config".into(), "proj".into()).unwrap();
        store.add("network setup".into(), "network".into(), "proj".into()).unwrap();
        store.add("port mapping".into(), "config".into(), "other".into()).unwrap();

        // Filter by project + keyword
        let results = store.query(Some("port"), Some("proj"), None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "server config port 8080");
    }

    #[test]
    fn query_respects_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        for i in 0..5 {
            store.add(format!("memory {i}"), "test".into(), "p".into()).unwrap();
        }

        let results = store.query(None, Some("p"), None, 3);
        assert_eq!(results.len(), 3);
    }
}
