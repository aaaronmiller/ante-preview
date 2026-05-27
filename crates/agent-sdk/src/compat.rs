//! Claude Code compatibility adapter.
//!
//! Reads `.claude/settings.json` and translates Claude Code hook
//! configurations into Ante hook rules. This allows users to reuse
//! their existing Claude Code hooks without modification.

use std::fs;
use std::path::PathBuf;

use ante_protocol_shape::settings::{
    ClaudeCompatFlags, HookDefinition, HookMatchRule,
};
use ante_protocol_shape::event::EventType;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompatError {
    #[error("Claude settings file not found at {0}")]
    NotFound(PathBuf),

    #[error("Failed to read Claude settings: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse Claude settings: {0}")]
    Parse(#[from] serde_json::Error),
}

/// Claude Code settings.json structure.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeSettings {
    #[serde(default)]
    hooks: Option<ClaudeHooksConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeHooksConfig {
    #[serde(default)]
    rules: Vec<ClaudeHookRule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeHookRule {
    #[serde(default)]
    event_types: Vec<String>,
    #[serde(default)]
    tool_name_pattern: Option<String>,
    #[serde(default)]
    hooks: Vec<ClaudeHookDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeHookDefinition {
    r#type: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    args: Option<Vec<String>>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    server: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    args_object: Option<serde_json::Value>,
}

/// Load Claude Code hooks from `.claude/settings.json`.
///
/// Returns an empty vector if the file doesn't exist or has no hooks.
pub fn load_claude_hooks(flags: &ClaudeCompatFlags) -> Result<Vec<HookMatchRule>, CompatError> {
    let path = flags
        .claude_settings_path
        .clone()
        .unwrap_or_else(default_claude_path);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)?;
    let settings: ClaudeSettings = serde_json::from_str(&content)?;

    let rules = settings
        .hooks
        .map(|h| h.rules)
        .unwrap_or_default()
        .into_iter()
        .filter_map(translate_rule)
        .collect();

    Ok(rules)
}

fn default_claude_path() -> PathBuf {
    if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        let candidate = PathBuf::from(config).join(".claude/settings.json");
        if candidate.exists() {
            return candidate;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".claude/settings.json");
    }
    PathBuf::from(".claude/settings.json")
}

fn translate_rule(claude_rule: ClaudeHookRule) -> Option<HookMatchRule> {
    let event_types: Vec<EventType> = claude_rule
        .event_types
        .iter()
        .filter_map(|name| claude_event_to_ante(name))
        .collect();

    if event_types.is_empty() {
        return None;
    }

    let hooks: Vec<HookDefinition> = claude_rule
        .hooks
        .into_iter()
        .filter_map(|h| match h.r#type.as_str() {
            "command" => h.command.map(|cmd| HookDefinition::Command {
                command: cmd,
                args: h.args.unwrap_or_default(),
                timeout_ms: h.timeout_ms,
            }),
            "prompt" => h.prompt.map(|prompt| HookDefinition::Prompt {
                prompt,
                model: h.model,
            }),
            "mcp_tool" => {
                let server = h.server?;
                let tool = h.tool?;
                let args = h
                    .args_object
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(k, v)| (k, v))
                    .collect();
                Some(HookDefinition::McpTool {
                    server,
                    tool,
                    args,
                })
            }
            _ => None,
        })
        .collect();

    if hooks.is_empty() {
        return None;
    }

    Some(HookMatchRule {
        event_types,
        tool_name_pattern: claude_rule.tool_name_pattern,
        hooks,
    })
}

/// Convert a Claude Code event name string to an Ante EventType.
fn claude_event_to_ante(name: &str) -> Option<EventType> {
    match name {
        "PreToolUse" => Some(EventType::PreToolUse),
        "PostToolUse" => Some(EventType::PostToolUse),
        "PostToolUseFailure" => Some(EventType::PostToolUseFailure),
        "PreUserPromptSubmit" => Some(EventType::PreUserPromptSubmit),
        "PostUserPromptSubmit" => Some(EventType::PostUserPromptSubmit),
        "SessionStart" => Some(EventType::SessionStart),
        "SessionEnd" => Some(EventType::SessionEnd),
        "PreCompact" => Some(EventType::PreCompact),
        "PostCompact" => Some(EventType::PostCompact),
        "PermissionRequest" => Some(EventType::PermissionRequest),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_claude_hooks_from_file() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let path = tmp.path().join("settings.json");
        let mut f = fs::File::create(&path).expect("create");
        f.write_all(
            br#"{
                "hooks": {
                    "rules": [{
                        "eventTypes": ["PreToolUse"],
                        "toolNamePattern": "Bash",
                        "hooks": [{"type": "command", "command": "/bin/true", "args": []}]
                    }]
                }
            }"#,
        )
        .expect("write");

        let flags = ClaudeCompatFlags {
            merge_claude_settings: true,
            claude_settings_path: Some(path),
            translate_event_names: true,
            write_claude_settings: false,
        };

        let rules = load_claude_hooks(&flags).expect("load");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].event_types.len(), 1);
        assert_eq!(rules[0].event_types[0], EventType::PreToolUse);
    }

    #[test]
    fn returns_empty_when_no_file() {
        let flags = ClaudeCompatFlags {
            merge_claude_settings: true,
            claude_settings_path: Some(PathBuf::from("/nonexistent/settings.json")),
            translate_event_names: true,
            write_claude_settings: false,
        };
        let rules = load_claude_hooks(&flags).expect("load");
        assert!(rules.is_empty());
    }

    #[test]
    fn translates_all_claude_event_names() {
        assert_eq!(
            claude_event_to_ante("PreToolUse"),
            Some(EventType::PreToolUse)
        );
        assert_eq!(
            claude_event_to_ante("PostToolUse"),
            Some(EventType::PostToolUse)
        );
        assert_eq!(
            claude_event_to_ante("SessionEnd"),
            Some(EventType::SessionEnd)
        );
        assert_eq!(claude_event_to_ante("UnknownEvent"), None);
    }
}
