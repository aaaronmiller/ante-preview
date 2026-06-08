//! Session logging and recovery for Ante.
//!
//! Records every session to a JSONL file in `~/.ante/sessions/` using a format
//! that is **fully compatible with cass** (coding-agent-session-search) via its
//! `PiAgentConnector`.
//!
//! ## File layout
//!
//! ```text
//! ~/.ante/sessions/
//!   ├── --safe-cwd-path--/
//!   │   ├── 2026-06-05T11-31-10-675Z_019e978d-26d3-7837-9281-1bc9074c3573.jsonl
//!   │   └── ...
//!   └── sessions_index.json   (lightweight registry for fast listing)
//! ```
//!
//! ## JSONL entry types (cass-compatible)
//!
//! - **`session`**: header with id, timestamp, cwd, provider, modelId, thinkingLevel.
//! - **`message`**: a conversation turn (user / assistant / toolResult).
//! - **`model_change`**: record when the model/provider changes mid-session.
//! - **`thinking_level_change`**: record thinking-level transitions.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ═════════════════════════════════════════════════════════════════════════════
// Safe-path helper
// ═════════════════════════════════════════════════════════════════════════════

/// Convert an arbitrary filesystem path into a safe directory name
/// that can be used as a folder name (e.g. `/home/user/project`
/// → `--home-user-project--`).
pub fn path_to_safe_name(cwd: &Path) -> String {
    let s = cwd.to_string_lossy().replace('/', "-");
    // Strip leading dash that comes from the root "/" → "-" conversion
    let trimmed = s.trim_start_matches('-');
    format!("--{trimmed}--")
}

/// Inverse of [`path_to_safe_name`].
///
/// Since absolute paths (starting with `/`) lose their leading `/` when
/// converted (it becomes `--home-user-project--`), we assume all paths
/// are absolute and prepend `/`.
pub fn safe_name_to_path(name: &str) -> Option<PathBuf> {
    let stripped = name.strip_prefix("--")?.strip_suffix("--")?;
    let path_str = format!("/{}", stripped.replace('-', "/"));
    Some(PathBuf::from(path_str))
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ═════════════════════════════════════════════════════════════════════════════
// Entry types written to the JSONL file
// ═════════════════════════════════════════════════════════════════════════════

/// A JSONL line written to the session file.
///
/// Uses `#[serde(tag = "type")]` for proper discriminant-based
/// deserialization so that `"type": "session"` maps to `Session`,
/// `"type": "message"` maps to `Message`, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionLine {
    #[serde(rename = "session")]
    Session(SessionHeader),
    #[serde(rename = "message")]
    Message(SessionMessageEnvelope),
    #[serde(rename = "model_change")]
    ModelChange(ModelChangeEntry),
    #[serde(rename = "thinking_level_change")]
    ThinkingLevelChange(ThinkingLevelChangeEntry),
}

/// Session header — written once as the first line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHeader {
    pub id: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
}

/// A message envelope (wraps the inner message for the JSONL line).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessageEnvelope {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub timestamp: String,
    pub message: SessionMessage,
}

/// The actual message body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String, // "user" | "assistant" | "toolResult"
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
}

/// Model change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelChangeEntry {
    pub provider: String,
    pub model_id: String,
    pub timestamp: String,
}

/// Thinking level change entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingLevelChangeEntry {
    pub level: String,
    pub timestamp: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Session Index (for fast listing without scanning all JSONL files)
// ═════════════════════════════════════════════════════════════════════════════

/// Lightweight index entry for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIndexEntry {
    /// The session UUID.
    pub session_id: String,
    /// Human-readable project directory.
    pub project: String,
    /// The safe-name subdirectory.
    pub safe_path: String,
    /// Relative path from sessions root to the JSONL file.
    pub file_path: String,
    /// ISO 8601 start timestamp.
    pub started_at: String,
    /// ISO 8601 end timestamp (if session has ended).
    pub ended_at: Option<String>,
    /// Provider name.
    pub provider: Option<String>,
    /// Model ID.
    pub model_id: Option<String>,
    /// Approximate message count.
    pub message_count: usize,
    /// Total tokens used (approximate).
    pub total_tokens: u64,
}

/// The on-disk index format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndex {
    pub sessions: Vec<SessionIndexEntry>,
}

impl SessionIndex {
    /// Load from file, or return empty index.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or(Self {
                sessions: Vec::new(),
            }),
            Err(_) => Self {
                sessions: Vec::new(),
            },
        }
    }

    /// Save to file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        // Atomic write via temp file + rename
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &json)?;
        fs::rename(tmp_path, path)?;
        Ok(())
    }

    /// Upsert (add or update) a session entry.
    pub fn upsert(&mut self, entry: SessionIndexEntry) {
        if let Some(existing) = self
            .sessions
            .iter_mut()
            .find(|e| e.session_id == entry.session_id)
        {
            *existing = entry;
        } else {
            self.sessions.push(entry);
        }
    }

    /// Return sessions sorted most-recent-first.
    pub fn sorted(&self) -> Vec<&SessionIndexEntry> {
        let mut sorted: Vec<_> = self.sessions.iter().collect();
        sorted.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        sorted
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Session Manager
// ═════════════════════════════════════════════════════════════════════════════

/// Manages session logging to a JSONL file.
///
/// ## Thread safety
///
/// The manager wraps a `Mutex<File>` so it can be shared across async tasks.
/// Each write is an atomic append + flush.
pub struct SessionManager {
    /// Root sessions directory (e.g. `~/.ante/sessions/`).
    sessions_root: PathBuf,
    /// The active JSONL file writer.
    writer: Mutex<Option<SessionFileWriter>>,
    /// Active session metadata.
    active: Mutex<Option<ActiveSession>>,
}

struct SessionFileWriter {
    file: fs::File,
    #[allow(dead_code)]
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ActiveSession {
    session_id: String,
    safe_path: String,
    rel_file_path: String,
    started_at: String,
    provider: Option<String>,
    model_id: Option<String>,
    cwd: PathBuf,
    message_count: usize,
    total_tokens: u64,
}

impl SessionManager {
    /// Create a new session manager rooted at the given `sessions_root`.
    pub fn new(sessions_root: PathBuf) -> Self {
        Self {
            sessions_root,
            writer: Mutex::new(None),
            active: Mutex::new(None),
        }
    }

    /// The default sessions root (`~/.ante/sessions/`).
    pub fn default_root() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".ante").join("sessions")
    }

    /// Index file path.
    pub fn index_path(&self) -> PathBuf {
        self.sessions_root.join("sessions_index.json")
    }

    // ── Session lifecycle ────────────────────────────────────────────────

    /// Start a new session and write the session header line.
    ///
    /// Returns the session UUID.
    pub fn start(
        &self,
        cwd: &Path,
        provider: Option<&str>,
        model_id: Option<&str>,
    ) -> io::Result<String> {
        let session_id = generate_session_id();
        let timestamp = iso_now();
        let safe_path = path_to_safe_name(cwd);
        let file_name = format!("{timestamp}_{session_id}.jsonl");
        let rel_path = format!("{safe_path}/{file_name}");

        let file_dir = self.sessions_root.join(&safe_path);
        fs::create_dir_all(&file_dir)?;
        let file_path = file_dir.join(&file_name);

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&file_path)?;

        // Write session header
        let header = SessionHeader {
            id: session_id.clone(),
            timestamp: timestamp.clone(),
            cwd: Some(cwd.to_string_lossy().to_string()),
            provider: provider.map(String::from),
            model_id: model_id.map(String::from),
            thinking_level: Some("normal".into()),
        };

        let line = serde_json::to_string(&SessionLine::Session(header))?;
        writeln!(file, "{line}")?;
        file.flush()?;

        // Store active session
        let active = ActiveSession {
            session_id: session_id.clone(),
            safe_path,
            rel_file_path: rel_path.clone(),
            started_at: timestamp,
            provider: provider.map(String::from),
            model_id: model_id.map(String::from),
            cwd: cwd.to_path_buf(),
            message_count: 0,
            total_tokens: 0,
        };

        *lock_or_recover(&self.writer) = Some(SessionFileWriter {
            file,
            path: file_path,
        });
        *lock_or_recover(&self.active) = Some(active);

        Ok(session_id)
    }

    /// Record a user message.
    pub fn record_user_message(&self, content: &str) -> io::Result<String> {
        let content_val = serde_json::Value::String(content.to_string());
        self.record_message("user", content_val, None, None, None, None)
    }

    /// Record an assistant message with optional model info and content blocks.
    pub fn record_assistant_message(
        &self,
        content_blocks: Value,
        model: Option<&str>,
        usage: Option<Value>,
    ) -> io::Result<String> {
        self.record_message("assistant", content_blocks, None, None, model, usage)
    }

    /// Record a tool result message.
    pub fn record_tool_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        content: Value,
        is_error: bool,
    ) -> io::Result<String> {
        self.record_message(
            "toolResult",
            content,
            Some(tool_call_id),
            Some(tool_name),
            None,
            None,
        )
        .map(|mut id| {
            if is_error {
                id.push_str(":error");
            }
            id
        })
    }

    /// Record a model change.
    pub fn record_model_change(&self, provider: &str, model_id: &str) -> io::Result<()> {
        let entry = ModelChangeEntry {
            provider: provider.into(),
            model_id: model_id.into(),
            timestamp: iso_now(),
        };
        self.write_line(&serde_json::to_string(&SessionLine::ModelChange(entry))?)?;
        // Update active session model info
        if let Some(ref active) = *lock_or_recover(&self.active) {
            let mut active = active.clone();
            active.provider = Some(provider.into());
            active.model_id = Some(model_id.into());
            // Can't update the Mutex<Option<ActiveSession>> directly without a new type
            // but the metadata in the JSONL file is what matters.
        }
        Ok(())
    }

    /// End the active session, finalize the index entry, and close the file.
    pub fn end(&self, total_tokens: u64) -> io::Result<()> {
        let active = {
            let guard = lock_or_recover(&self.active);
            guard.clone()
        };

        if let Some(active) = active {
            let ended_at = iso_now();

            // Update index
            let mut index = SessionIndex::load(&self.index_path());
            index.upsert(SessionIndexEntry {
                session_id: active.session_id,
                project: active.cwd.to_string_lossy().to_string(),
                safe_path: active.safe_path,
                file_path: active.rel_file_path,
                started_at: active.started_at,
                ended_at: Some(ended_at),
                provider: active.provider,
                model_id: active.model_id,
                message_count: active.message_count,
                total_tokens,
            });
            index.save(&self.index_path())?;
        }

        // Close the file
        *lock_or_recover(&self.writer) = None;
        *lock_or_recover(&self.active) = None;

        Ok(())
    }

    /// Track token usage incrementally on the active session.
    pub fn add_tokens(&self, tokens: u64) {
        if let Some(ref mut active) = *lock_or_recover(&self.active) {
            active.total_tokens += tokens;
            active.message_count += 1;
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Session query / recovery
    // ══════════════════════════════════════════════════════════════════════

    /// List all sessions from the index, most recent first.
    pub fn list_sessions(&self) -> io::Result<Vec<SessionIndexEntry>> {
        let index = SessionIndex::load(&self.index_path());
        Ok(index.sorted().into_iter().cloned().collect())
    }

    /// List sessions for a specific project path, most recent first.
    pub fn list_sessions_for_project(&self, cwd: &Path) -> io::Result<Vec<SessionIndexEntry>> {
        let cwd_str = cwd.to_string_lossy().to_string();
        let index = SessionIndex::load(&self.index_path());
        let mut matches: Vec<_> = index
            .sessions
            .into_iter()
            .filter(|e| e.project == cwd_str)
            .collect();
        matches.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(matches)
    }

    /// Get the most recent session for a project.
    pub fn latest_session_for_project(&self, cwd: &Path) -> io::Result<Option<SessionIndexEntry>> {
        let sessions = self.list_sessions_for_project(cwd)?;
        Ok(sessions.into_iter().next())
    }

    /// Read the full content of a session file (for recovery / display).
    pub fn read_session(&self, session_id: &str) -> io::Result<Option<Vec<SessionLine>>> {
        let index = SessionIndex::load(&self.index_path());
        let entry = match index.sessions.iter().find(|e| e.session_id == session_id) {
            Some(e) => e,
            None => return Ok(None),
        };

        let file_path = self.sessions_root.join(&entry.file_path);
        if !file_path.exists() {
            return Ok(None);
        }

        let file = fs::File::open(&file_path)?;
        let reader = io::BufReader::new(file);
        let mut lines = Vec::new();

        for line_result in reader.lines() {
            let line = line_result?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<SessionLine>(&line) {
                lines.push(parsed);
            }
        }

        Ok(Some(lines))
    }

    /// Recover the conversation context from the last session for a project.
    /// Returns the last N messages formatted as a context string that can be
    /// injected into the new session.
    pub fn recover_context(
        &self,
        cwd: &Path,
        max_messages: usize,
    ) -> io::Result<Option<(String, String)>> {
        // (context_string, session_id)
        let entry = match self.latest_session_for_project(cwd)? {
            Some(e) => e,
            None => return Ok(None),
        };

        let lines = match self.read_session(&entry.session_id)? {
            Some(l) => l,
            None => return Ok(None),
        };

        // Extract messages (skip header, model changes, etc.)
        let messages: Vec<&SessionMessageEnvelope> = lines
            .iter()
            .filter_map(|line| match line {
                SessionLine::Message(msg) => Some(msg),
                _ => None,
            })
            .collect();

        // Take the last N messages
        let recent: Vec<&&SessionMessageEnvelope> =
            messages.iter().rev().take(max_messages).rev().collect();

        if recent.is_empty() {
            return Ok(None);
        }

        let mut ctx = format!(
            "\n[Previous session {} — recent context]\n",
            entry.session_id
        );

        for msg in recent {
            let role = &msg.message.role;
            let content_str = match &msg.message.content {
                Value::String(s) => s.clone(),
                Value::Array(arr) => {
                    let parts: Vec<String> = arr
                        .iter()
                        .filter_map(|block| match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                block.get("text").and_then(|t| t.as_str()).map(String::from)
                            }
                            Some("thinking") => block
                                .get("thinking")
                                .and_then(|t| t.as_str())
                                .map(|t| format!("[thinking] {t}")),
                            Some("toolCall") => {
                                let name =
                                    block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                Some(format!("[tool: {name}]"))
                            }
                            _ => None,
                        })
                        .collect();
                    parts.join("\n")
                }
                other => other.to_string(),
            };

            if !content_str.trim().is_empty() {
                ctx.push_str(&format!("  [{role}] {content_str}\n"));
            }
        }

        ctx.push_str("[/Previous session context]\n");

        Ok(Some((ctx, entry.session_id.clone())))
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn record_message(
        &self,
        role: &str,
        content: Value,
        tool_call_id: Option<&str>,
        tool_name: Option<&str>,
        model: Option<&str>,
        usage: Option<Value>,
    ) -> io::Result<String> {
        let id = generate_message_id();
        let timestamp = iso_now();

        // Determine parent ID from the active session's last message
        let parent_id = None; // Could track message chain if needed

        let msg = SessionMessage {
            role: role.into(),
            content,
            tool_call_id: tool_call_id.map(String::from),
            tool_name: tool_name.map(String::from),
            is_error: None,
            model: model.map(String::from),
            usage,
        };

        let envelope = SessionMessageEnvelope {
            id: id.clone(),
            parent_id,
            timestamp,
            message: msg,
        };

        let line = serde_json::to_string(&SessionLine::Message(envelope))?;
        self.write_line(&line)?;

        // Bump message count on active session
        if let Some(ref mut active) = *lock_or_recover(&self.active) {
            active.message_count += 1;
        }

        Ok(id)
    }

    fn write_line(&self, json_line: &str) -> io::Result<()> {
        let guard = lock_or_recover(&self.writer);
        if let Some(ref writer) = *guard {
            let mut file = &writer.file;
            writeln!(file, "{json_line}")?;
            file.flush()?;
        }
        Ok(())
    }

    /// Check if a session is currently active.
    pub fn is_active(&self) -> bool {
        lock_or_recover(&self.active).is_some()
    }

    /// Get the active session ID, if any.
    pub fn active_session_id(&self) -> Option<String> {
        lock_or_recover(&self.active)
            .as_ref()
            .map(|a| a.session_id.clone())
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// ID & timestamp generators
// ═════════════════════════════════════════════════════════════════════════════

/// Generate a UUID v4 for session identification.
///
/// Format: `019e978d-26d3-7837-9281-1bc9074c3573`
fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a short message ID (8 hex chars).
fn generate_message_id() -> String {
    let uuid = Uuid::new_v4();
    uuid.to_string()[..8].to_string()
}

/// Current timestamp in ISO 8601 format.
fn iso_now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_safe_name_converts() {
        let name = path_to_safe_name(Path::new("/home/user/my-project"));
        assert_eq!(name, "--home-user-my-project--");
    }

    #[test]
    fn safe_name_roundtrip() {
        let name = path_to_safe_name(Path::new("/home/user/project"));
        let back = safe_name_to_path(&name);
        assert_eq!(back, Some(PathBuf::from("/home/user/project")));
    }

    #[test]
    fn session_start_writes_header() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let cwd = Path::new("/home/user/project");

        let session_id = mgr.start(cwd, Some("anthropic"), Some("claude-3")).unwrap();

        // Check that a JSONL file was created
        let safe = path_to_safe_name(cwd);
        let session_dir = dir.path().join(&safe);
        assert!(session_dir.exists());

        // Read the file and verify the header
        let entries: Vec<PathBuf> = fs::read_dir(&session_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        assert_eq!(entries.len(), 1);

        let content = fs::read_to_string(&entries[0]).unwrap();
        let first_line = content.lines().next().unwrap();
        let parsed: Value = serde_json::from_str(first_line).unwrap();
        assert_eq!(parsed["type"], "session");
        assert_eq!(parsed["id"], session_id);
        assert_eq!(parsed["cwd"], "/home/user/project");
        assert_eq!(parsed["provider"], "anthropic");
        assert_eq!(parsed["modelId"], "claude-3");
    }

    #[test]
    fn record_messages_appends_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let cwd = Path::new("/home/user/project");

        mgr.start(cwd, None, None).unwrap();
        mgr.record_user_message("Hello!").unwrap();
        mgr.record_assistant_message(
            serde_json::json!([{"type": "text", "text": "Hi there!"}]),
            Some("claude-3"),
            None,
        )
        .unwrap();
        mgr.record_tool_result(
            "call-1",
            "read",
            serde_json::json!([{"type": "text", "text": "file content"}]),
            false,
        )
        .unwrap();
        mgr.end(150).unwrap();

        // Read back and verify
        let safe = path_to_safe_name(cwd);
        let session_dir = dir.path().join(&safe);
        let file_path = fs::read_dir(&session_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap()
            .path();

        let content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 4); // header + 3 messages

        // Verify message entries
        let second_line: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second_line["type"], "message");
        assert_eq!(second_line["message"]["role"], "user");

        let third_line: Value = serde_json::from_str(lines[2]).unwrap();
        assert_eq!(third_line["message"]["role"], "assistant");

        let fourth_line: Value = serde_json::from_str(lines[3]).unwrap();
        assert_eq!(fourth_line["message"]["role"], "toolResult");
        assert_eq!(fourth_line["message"]["toolName"], "read");
    }

    #[test]
    fn index_updated_on_end() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let cwd = Path::new("/home/user/project");

        let session_id = mgr.start(cwd, Some("openai"), Some("gpt-4")).unwrap();
        mgr.record_user_message("Test").unwrap();
        mgr.end(500).unwrap();

        let index = SessionIndex::load(&mgr.index_path());
        assert_eq!(index.sessions.len(), 1);
        assert_eq!(index.sessions[0].session_id, session_id);
        assert!(index.sessions[0].ended_at.is_some());
        assert_eq!(index.sessions[0].message_count, 1); // 1 user message recorded
    }

    #[test]
    fn list_sessions_for_project() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());

        let cwd1 = Path::new("/home/user/proj1");
        mgr.start(cwd1, None, None).unwrap();
        mgr.end(100).unwrap();

        let cwd2 = Path::new("/home/user/proj2");
        mgr.start(cwd2, None, None).unwrap();
        mgr.end(200).unwrap();

        let for_proj1 = mgr.list_sessions_for_project(cwd1).unwrap();
        assert_eq!(for_proj1.len(), 1);

        let all = mgr.list_sessions().unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn recover_context_returns_last_messages() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let cwd = Path::new("/home/user/project");

        mgr.start(cwd, None, None).unwrap();
        mgr.record_user_message("What's the weather?").unwrap();
        mgr.record_assistant_message(
            serde_json::json!([{"type": "text", "text": "It's sunny!"}]),
            None,
            None,
        )
        .unwrap();
        mgr.end(100).unwrap();

        let (ctx, session_id) = mgr.recover_context(cwd, 10).unwrap().unwrap();
        assert!(ctx.contains("What's the weather?"));
        assert!(ctx.contains("It's sunny!"));
        assert!(ctx.contains(&session_id));
    }

    #[test]
    fn empty_recovery_for_no_session() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let result = mgr
            .recover_context(Path::new("/home/user/other"), 10)
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn model_change_writes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let mgr = SessionManager::new(dir.path().to_path_buf());
        let cwd = Path::new("/home/user/project");

        mgr.start(cwd, Some("openai"), Some("gpt-4")).unwrap();
        mgr.record_model_change("anthropic", "claude-3").unwrap();
        mgr.end(0).unwrap();

        let safe = path_to_safe_name(cwd);
        let session_dir = dir.path().join(&safe);
        let file_path = fs::read_dir(&session_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .next()
            .unwrap()
            .path();

        let content = fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        let change_line: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(change_line["type"], "model_change");
        assert_eq!(change_line["provider"], "anthropic");
        assert_eq!(change_line["modelId"], "claude-3");
    }
}
