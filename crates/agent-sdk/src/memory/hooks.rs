//! Memory hooks — auto-store tool results as session memories.
//!
//! When a tool succeeds, its name and output are summarised and stored
//! as a memory entry so the agent can recall them across sessions.

use ante_protocol_shape::{EventPayload, ToolUsePayload};

use super::store::MemoryStore;

/// Configuration for memory hook behaviour.
#[derive(Debug, Clone)]
pub struct MemoryHookConfig {
    /// Project name to tag memories with.
    pub project: String,
    /// Skip storing memory for tool calls whose output is shorter than this.
    pub min_output_chars: usize,
    /// Tool names to exclude from auto-memory (e.g. "Read", "View").
    pub exclude_tools: Vec<String>,
    /// Whether to store tool inputs alongside outputs.
    pub include_input: bool,
}

impl Default for MemoryHookConfig {
    fn default() -> Self {
        Self {
            project: "default".into(),
            min_output_chars: 20,
            exclude_tools: vec!["View".into(), "ListFiles".into()],
            include_input: false,
        }
    }
}

/// Handle a PostToolUse event by extracting tool output and storing
/// it as a tagged memory entry.
///
/// Returns `true` if a memory was stored, `false` if the event was
/// filtered out (too short, excluded tool, etc.).
pub fn handle_post_tool_use(
    payload: &EventPayload,
    store: &mut MemoryStore,
    config: &MemoryHookConfig,
) -> bool {
    // Only process PostToolUse events
    let tool_payload = match payload {
        EventPayload::PostToolUse(p) => p,
        _ => return false,
    };

    // Check excluded tools
    if config
        .exclude_tools
        .iter()
        .any(|t| t.eq_ignore_ascii_case(&tool_payload.tool_name))
    {
        return false;
    }

    // Extract output text
    let output_text = extract_output_text(tool_payload);
    if output_text.len() < config.min_output_chars {
        return false;
    }

    // Build content string
    let mut content = format!(
        "[tool:{}] {}",
        tool_payload.tool_name,
        truncate_to(&output_text, 500)
    );

    if config.include_input {
        let input_summary = serde_json::to_string(&tool_payload.input)
            .unwrap_or_default();
        if input_summary.len() > 5 {
            content.push_str(&format!("\ninput: {}", truncate_to(&input_summary, 200)));
        }
    }

    // Store as memory
    let tags = format!("tool,{}", tool_payload.tool_name.to_lowercase());
    match store.add(content, tags, config.project.clone()) {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Extract human-readable output text from a tool payload.
fn extract_output_text(payload: &ToolUsePayload) -> String {
    if let Some(ref err) = payload.error {
        return format!("error: {err}");
    }
    match &payload.output {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(obj)) => {
            obj.get("content")
                .and_then(|c| c.as_str())
                .or_else(|| obj.get("text").and_then(|t| t.as_str()))
                .or_else(|| obj.get("output").and_then(|o| o.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| serde_json::to_string(obj).unwrap_or_default())
        }
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

use serde_json::Value;

/// Truncate to a max length, appending "..." if truncated.
fn truncate_to(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut truncated = s[..max.saturating_sub(3)].to_string();
        truncated.push_str("...");
        truncated
    }
}

/// Handle a PostToolUseFailure event — store error as a memory tag.
pub fn handle_post_tool_failure(
    payload: &EventPayload,
    store: &mut MemoryStore,
    config: &MemoryHookConfig,
) -> bool {
    let tool_payload = match payload {
        EventPayload::PostToolUseFailure(p) => p,
        _ => return false,
    };

    let error_msg = tool_payload
        .error
        .as_deref()
        .unwrap_or("unknown error");
    let content = format!(
        "[tool:{} failure] {}",
        tool_payload.tool_name, error_msg
    );
    let tags = format!("tool,failure,{}", tool_payload.tool_name.to_lowercase());

    match store.add(content, tags, config.project.clone()) {
        Ok(_) => true,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ante_protocol_shape::BasePayload;
    use std::path::PathBuf;

    fn make_post_tool_payload(tool: &str, output: Option<&str>) -> EventPayload {
        EventPayload::PostToolUse(ToolUsePayload {
            base: BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into()),
            tool_name: tool.into(),
            input: serde_json::json!({"command": "test"}),
            output: output.map(|s| serde_json::json!({"content": s})),
            error: None,
            duration_ms: Some(100),
        })
    }

    fn make_failure_payload(tool: &str, error: &str) -> EventPayload {
        EventPayload::PostToolUseFailure(ToolUsePayload {
            base: BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into()),
            tool_name: tool.into(),
            input: serde_json::json!({"command": "test"}),
            output: None,
            error: Some(error.into()),
            duration_ms: Some(50),
        })
    }

    #[test]
    fn stores_successful_tool_as_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let config = MemoryHookConfig {
            project: "test".into(),
            min_output_chars: 5,
            exclude_tools: vec![],
            include_input: false,
        };

        let payload = make_post_tool_payload("Bash", Some("hello world this is output"));
        let stored = handle_post_tool_use(&payload, &mut store, &config);
        assert!(stored);
        assert_eq!(store.count(), 1);
        assert!(store.search("Bash").len() == 1);
    }

    #[test]
    fn filters_short_output() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let config = MemoryHookConfig {
            project: "test".into(),
            min_output_chars: 100,
            exclude_tools: vec![],
            include_input: false,
        };

        let payload = make_post_tool_payload("Bash", Some("short"));
        let stored = handle_post_tool_use(&payload, &mut store, &config);
        assert!(!stored);
    }

    #[test]
    fn filters_excluded_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let config = MemoryHookConfig {
            project: "test".into(),
            min_output_chars: 1,
            exclude_tools: vec!["Bash".into()],
            include_input: false,
        };

        let payload = make_post_tool_payload("Bash", Some("some output"));
        let stored = handle_post_tool_use(&payload, &mut store, &config);
        assert!(!stored);
    }

    #[test]
    fn stores_failure_as_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let config = MemoryHookConfig::default();

        let payload = make_failure_payload("Bash", "command not found");
        let stored = handle_post_tool_failure(&payload, &mut store, &config);
        assert!(stored);
        assert!(store.search("failure").len() == 1);
    }

    #[test]
    fn ignores_non_post_tool_events() {
        let tmp = tempfile::tempdir().unwrap();
        let mut store = MemoryStore::open(tmp.path().join("m.json"), 20).unwrap();
        let config = MemoryHookConfig::default();

        // SessionStart payload — shouldn't match
        let payload = EventPayload::SessionStart(ante_protocol_shape::SessionStartPayload {
            base: BasePayload::new(PathBuf::from("/tmp"), "0.2.0".into()),
            session_id: ante_protocol_shape::Id::ses(),
            model: "test".into(),
            provider: "test".into(),
            project_dir: None,
            project_name: None,
        });
        let stored = handle_post_tool_use(&payload, &mut store, &config);
        assert!(!stored);
    }
}
