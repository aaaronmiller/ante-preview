//! Settings loader — parses `~/.ante/settings.json` and merges
//! with `.claude/settings.json` when Claude Code compat is enabled.

use std::fs;
use std::path::{Path, PathBuf};

use ante_protocol_shape::Settings;
use ante_protocol_shape::event::EventType;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadSettingsError {
    #[error("Settings file not found at {0} — using defaults")]
    NotFound(PathBuf),

    #[error("Failed to read settings file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to parse settings file {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// Load Ante settings from the default path (`~/.ante/settings.json`).
///
/// Returns `Ok(Settings::default())` if the file doesn't exist
/// (graceful first-run behaviour).
pub fn load_settings() -> Result<Settings, LoadSettingsError> {
    let ante_dir = default_ante_dir();
    let path = ante_dir.join("settings.json");
    load_settings_from(&path)
}

/// Load settings from a specific path, with optional Claude Code merge.
pub fn load_settings_from(path: &Path) -> Result<Settings, LoadSettingsError> {
    let mut settings: Settings = read_settings_file(path)?;
    normalize_legacy_paths(&mut settings);

    if settings.claude_compat.merge_claude_settings {
        let claude_path = settings
            .claude_compat
            .claude_settings_path
            .clone()
            .unwrap_or_else(default_claude_settings_path);

        if claude_path.exists() {
            match read_settings_file::<serde_json::Value>(&claude_path) {
                Ok(claude_raw) => {
                    merge_claude_hooks(&mut settings, &claude_raw);
                }
                Err(LoadSettingsError::NotFound(_)) => {
                    // Claude settings not present — skip silently
                }
                Err(e) => {
                    // Log but don't fail — Claude compat is best-effort
                    eprintln!("[ante] warning: failed to load Claude Code settings: {e}");
                }
            }
        }
    }

    Ok(settings)
}

fn normalize_legacy_paths(settings: &mut Settings) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let Some(home) = home else {
        return;
    };

    let legacy_memory = home.join(".ante").join("memory").join("ante-memory.db");
    let wiki_memory_repo = home.join("code").join("wiki-memory");
    let wiki_memory_db = wiki_memory_repo
        .join("wiki")
        .join(".meta")
        .join("ante-memory.db");
    if settings.memory.db_path == legacy_memory {
        settings.memory.db_path = if wiki_memory_repo.exists() {
            wiki_memory_db.clone()
        } else {
            home.join("ai-wiki").join(".meta").join("ante-memory.db")
        };
    }

    if settings.memory.db_path == PathBuf::from("~/.ante/memory/ante-memory.db")
        || settings.memory.db_path == PathBuf::from("~/ai-wiki/.meta/ante-memory.db")
    {
        settings.memory.db_path = if wiki_memory_repo.exists() {
            wiki_memory_db
        } else {
            PathBuf::from("~/ai-wiki/.meta/ante-memory.db")
        };
    }
}

fn default_ante_dir() -> PathBuf {
    dirs_or_home(&[".ante"])
}

fn default_claude_settings_path() -> PathBuf {
    dirs_or_home(&[".claude", "settings.json"])
}

/// Walk `XDG_CONFIG_HOME` or `HOME` to find a config directory.
fn dirs_or_home(components: &[&str]) -> PathBuf {
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME") {
        let base = PathBuf::from(config);
        let candidate: PathBuf = components.iter().collect();
        let full = base.join(&candidate);
        if full.exists() {
            return full;
        }
    }

    let mut base = dirs_or_fallback();
    for c in components {
        base = base.join(c);
    }
    base
}

fn dirs_or_fallback() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    }
}

fn read_settings_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, LoadSettingsError> {
    if !path.exists() {
        return Err(LoadSettingsError::NotFound(path.to_path_buf()));
    }

    let content = fs::read_to_string(path).map_err(|source| LoadSettingsError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    serde_json::from_str(&content).map_err(|source| LoadSettingsError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Merge `hooks.rules` from a Claude Code settings.json into Ante settings.
///
/// This translates Claude Code event names to Ante event types when
/// `claude_compat.translate_event_names` is true.
fn merge_claude_hooks(ante: &mut Settings, claude: &serde_json::Value) {
    use ante_protocol_shape::{HookDefinition, HookMatchRule, event::EventType};

    // Claude Code hook config lives under `hooks.rules[].hooks`
    let Some(rules) = claude
        .get("hooks")
        .and_then(|h| h.get("rules"))
        .and_then(|r| r.as_array())
    else {
        return;
    };

    let translated: Vec<HookMatchRule> = rules
        .iter()
        .filter_map(|rule| {
            let event_types: Vec<EventType> = rule
                .get("eventTypes")
                .and_then(|et| et.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .filter_map(claude_event_to_ante)
                        .collect()
                })
                .unwrap_or_default();

            if event_types.is_empty() {
                return None;
            }

            let tool_name_pattern = rule
                .get("toolNamePattern")
                .and_then(|v| v.as_str())
                .map(String::from);

            let hooks: Vec<HookDefinition> = rule
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|h| {
                            let r#type = h.get("type").and_then(|v| v.as_str())?;
                            match r#type {
                                "command" => Some(HookDefinition::Command {
                                    command: h.get("command")?.as_str()?.to_string(),
                                    args: h
                                        .get("args")
                                        .and_then(|a| a.as_array())
                                        .map(|a| {
                                            a.iter()
                                                .filter_map(|v| v.as_str().map(String::from))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    timeout_ms: h.get("timeoutMs").and_then(|v| v.as_u64()),
                                }),
                                "prompt" => Some(HookDefinition::Prompt {
                                    prompt: h.get("prompt")?.as_str()?.to_string(),
                                    model: h
                                        .get("model")
                                        .and_then(|v| v.as_str().map(String::from)),
                                }),
                                "mcp_tool" => Some(HookDefinition::McpTool {
                                    server: h.get("server")?.as_str()?.to_string(),
                                    tool: h.get("tool")?.as_str()?.to_string(),
                                    args: h
                                        .get("args")
                                        .and_then(|a| a.as_object())
                                        .map(|o| {
                                            o.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                                        })
                                        .unwrap_or_default(),
                                }),
                                _ => None,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            Some(HookMatchRule {
                event_types,
                tool_name_pattern,
                hooks,
            })
        })
        .collect();

    ante.hooks.rules.extend(translated);
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
    fn load_settings_defaults_when_missing() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let path = tmp.path().join("settings.json");
        // Don't create the file
        let result = load_settings_from(&path);
        assert!(matches!(result, Err(LoadSettingsError::NotFound(_))));
    }

    #[test]
    fn load_settings_valid_json() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let path = tmp.path().join("settings.json");
        let mut f = fs::File::create(&path).expect("create file");
        f.write_all(
            br#"{
                "hooks": {
                    "rules": [{
                        "eventTypes": ["pre_tool_use"],
                        "toolNamePattern": "Bash",
                        "hooks": [
                            {"type": "command", "command": "/bin/true", "args": []}
                        ]
                    }],
                    "maxDepth": 3
                },
                "sensitiveTools": ["Bash", "Write"]
            }"#,
        )
        .expect("write");

        let settings = load_settings_from(&path).expect("load");
        assert_eq!(settings.hooks.rules.len(), 1);
        assert_eq!(settings.hooks.max_depth, 3);
        assert_eq!(settings.sensitive_tools.len(), 2);
    }

    #[test]
    fn merge_claude_hooks_appends_hooks() {
        let mut settings = Settings::default();
        settings.claude_compat.merge_claude_settings = true;

        let claude_json: serde_json::Value = serde_json::from_str(
            r#"{
                "hooks": {
                    "rules": [{
                        "eventTypes": ["PreToolUse"],
                        "toolNamePattern": "Write",
                        "hooks": [{"type": "command", "command": "echo blocked"}]
                    }]
                }
            }"#,
        )
        .expect("parse claude json");

        merge_claude_hooks(&mut settings, &claude_json);
        assert_eq!(settings.hooks.rules.len(), 1);
        assert_eq!(
            settings.hooks.rules[0].tool_name_pattern.as_deref(),
            Some("Write")
        );
    }

    #[test]
    fn claude_event_translation() {
        assert_eq!(
            claude_event_to_ante("PreToolUse"),
            Some(EventType::PreToolUse)
        );
        assert_eq!(claude_event_to_ante("UnknownEvent"), None);
    }
}
