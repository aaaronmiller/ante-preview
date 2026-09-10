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
// ─── Named profiles ───────────────────────────────────────────────────────────

/// Built-in minimal profile: plain agent with the extensibility layer off.
pub const BARE_PROFILE_NAME: &str = "bare";

/// Profile names: lowercase ASCII letters, digits, `-`, `_` (max 64 chars).
pub fn is_valid_profile_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Resolve `<name>` to `~/.ante/<name>.settings.json`.
///
/// - Invalid names → `None`.
/// - `bare` → seeds the built-in template on first use.
/// - Unknown names → `None` (caller warns and falls back).
pub fn resolve_profile_path(name: &str) -> Option<PathBuf> {
    resolve_profile_path_in(&default_ante_dir(), name)
}

fn resolve_profile_path_in(ante_dir: &Path, name: &str) -> Option<PathBuf> {
    if !is_valid_profile_name(name) {
        return None;
    }
    if name == BARE_PROFILE_NAME {
        seed_bare_profile_in(ante_dir);
    }
    let path = ante_dir.join(format!("{name}.settings.json"));
    path.exists().then_some(path)
}

fn seed_bare_profile_in(ante_dir: &Path) {
    let path = ante_dir.join(format!("{BARE_PROFILE_NAME}.settings.json"));
    if path.exists() {
        return;
    }
    // Best-effort: the user can edit the seeded file afterwards.
    let _ = fs::create_dir_all(ante_dir);
    let _ = fs::write(&path, "{\n  \"extensibilityEnabled\": false\n}\n");
}

/// Load settings, honouring `--profile <name>` / `ANTE_PROFILE`.
///
/// `None` → normal `load_settings()`. Unknown or invalid names warn on
/// stderr and fall back to the default settings file.
pub fn load_settings_with_profile(profile: Option<&str>) -> Result<Settings, LoadSettingsError> {
    match profile {
        None => load_settings(),
        Some(name) => match resolve_profile_path(name) {
            Some(path) => load_settings_from(&path),
            None => {
                eprintln!("[ante] warning: unknown profile '{name}', using default settings");
                load_settings()
            }
        },
    }
}

// ─── Project settings layer ───────────────────────────────────────────────────

/// Keys a project file may never set. Dropped with a notice: a repository
/// must not launch new server processes on the operator's machine.
const PROJECT_DROPPED_KEYS: &[&str] = &["mcpServers"];
/// Device-local keys a project file may not touch. Ignored with a notice.
const PROJECT_IGNORED_KEYS: &[&str] = &["anteDir"];

/// Outcome of applying the project layer (surfaced by `ante doctor`).
#[derive(Debug, Default)]
pub struct ProjectLayerOutcome {
    /// The project file that was applied, if any.
    pub path: Option<PathBuf>,
    /// Keys dropped for safety, in file order of the drop list.
    pub dropped: Vec<String>,
    /// Device-local keys ignored, in file order of the ignore list.
    pub ignored: Vec<String>,
}

/// Layer the nearest ancestor `.ante/settings.json` over `settings`.
///
/// - `mcpServers` is dropped (pin/narrow-only: a repo cannot widen).
/// - `anteDir` is ignored (device-local).
/// - `sensitiveTools` is unioned (a repo may only add restrictions).
/// - Every other key overlays the user/profile value.
/// Unreadable or invalid project files warn and leave `settings` untouched.
pub fn apply_project_layer(settings: &mut Settings, cwd: &Path) -> ProjectLayerOutcome {
    let mut outcome = ProjectLayerOutcome::default();
    let Some(path) = find_project_settings(cwd, &default_ante_dir().join("settings.json"))
    else {
        return outcome;
    };
    outcome.path = Some(path.clone());
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "[ante] warning: cannot read project settings {}: {e}",
                path.display()
            );
            return outcome;
        }
    };
    let project: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "[ante] warning: invalid project settings {}: {e}",
                path.display()
            );
            return outcome;
        }
    };
    let Some(obj) = project.as_object().cloned() else {
        eprintln!(
            "[ante] warning: project settings {} is not a JSON object",
            path.display()
        );
        return outcome;
    };
    let mut obj = obj;
    for key in PROJECT_DROPPED_KEYS {
        if obj.remove(*key).is_some() {
            eprintln!("[ante] notice: project settings may not set '{key}'; dropped");
            outcome.dropped.push(key.to_string());
        }
    }
    for key in PROJECT_IGNORED_KEYS {
        if obj.remove(*key).is_some() {
            eprintln!("[ante] notice: project settings ignores device-local '{key}'");
            outcome.ignored.push(key.to_string());
        }
    }
    let mut merged = serde_json::to_value(&*settings).unwrap_or(serde_json::Value::Null);
    if let Some(extra) = obj.get("sensitiveTools") {
        let mut union: Vec<serde_json::Value> = merged
            .get("sensitiveTools")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for v in extra.as_array().cloned().unwrap_or_default() {
            if !union.contains(&v) {
                union.push(v);
            }
        }
        merged["sensitiveTools"] = serde_json::Value::Array(union);
    }
    for (k, v) in obj.iter() {
        if k == "sensitiveTools" {
            continue;
        }
        merged[k] = v.clone();
    }
    match serde_json::from_value::<Settings>(merged) {
        Ok(layered) => {
            *settings = layered;
            normalize_legacy_paths(settings);
        }
        Err(e) => eprintln!("[ante] warning: project settings failed to apply: {e}"),
    }
    outcome
}

/// Walk up from `cwd` to find `.ante/settings.json`, skipping `exclude_file`
/// (the user's own settings file also matches the pattern when `cwd` sits
/// under the home directory).
fn find_project_settings(cwd: &Path, exclude_file: &Path) -> Option<PathBuf> {
    let mut dir = Some(cwd);
    while let Some(d) = dir {
        let candidate = d.join(".ante").join("settings.json");
        if candidate.is_file() && candidate != exclude_file {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    fn tmp_ante() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn profile_name_validation() {
        assert!(is_valid_profile_name("work"));
        assert!(is_valid_profile_name("a-b_c9"));
        assert!(!is_valid_profile_name(""));
        assert!(!is_valid_profile_name("Work"));
        assert!(!is_valid_profile_name("a/b"));
        assert!(!is_valid_profile_name("a b"));
    }

    #[test]
    fn unknown_profile_resolves_none() {
        let dir = tmp_ante();
        assert!(resolve_profile_path_in(dir.path(), "nope").is_none());
        assert!(resolve_profile_path_in(dir.path(), "Bogus!").is_none());
    }

    #[test]
    fn bare_profile_seeds_template() {
        let dir = tmp_ante();
        let path = resolve_profile_path_in(dir.path(), "bare").expect("bare seeds itself");
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("extensibilityEnabled"));
    }

    #[test]
    fn named_profile_loads_whole_file() {
        let dir = tmp_ante();
        fs::write(
            dir.path().join("work.settings.json"),
            r#"{"sensitiveTools": ["Bash", "Exec"]}"#,
        )
        .unwrap();
        let path = resolve_profile_path_in(dir.path(), "work").unwrap();
        let loaded = load_settings_from(&path).unwrap();
        // Whole-file replacement: omitted keys fall back to defaults.
        assert_eq!(loaded.sensitive_tools, vec!["Bash".to_string(), "Exec".to_string()]);
        assert!(loaded.mcp_servers.is_empty());
    }

    #[test]
    fn project_layer_drops_mcp_unions_tools_overlays_rest() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        let sub = repo.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(repo.join(".ante")).unwrap();
        fs::write(
            repo.join(".ante").join("settings.json"),
            r#"{
                "mcpServers": [{"name": "evil"}],
                "anteDir": "/tmp/x",
                "sensitiveTools": ["Exec"],
                "extensibilityEnabled": false
            }"#,
        )
        .unwrap();
        let mut settings = Settings::default();
        let outcome = apply_project_layer(&mut settings, &sub);
        assert_eq!(
            outcome.path,
            Some(repo.join(".ante").join("settings.json"))
        );
        assert_eq!(outcome.dropped, vec!["mcpServers".to_string()]);
        assert_eq!(outcome.ignored, vec!["anteDir".to_string()]);
        assert!(settings.mcp_servers.is_empty());
        assert_eq!(
            settings.sensitive_tools,
            vec!["Bash".to_string(), "Write".to_string(), "Exec".to_string()]
        );
        assert!(!settings.extensibility_enabled);
    }

    #[test]
    fn project_layer_absent_without_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        let outcome = apply_project_layer(&mut settings, dir.path());
        assert!(outcome.path.is_none());
        assert!(settings.extensibility_enabled);
    }
}

#[cfg(test)]
mod project_exclusion_tests {
    use super::*;

    #[test]
    fn project_search_skips_excluded_file() {
        let dir = tempfile::tempdir().unwrap();
        let ante = dir.path().join(".ante");
        fs::create_dir_all(&ante).unwrap();
        let settings_file = ante.join("settings.json");
        fs::write(&settings_file, r#"{"extensibilityEnabled": false}"#).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        assert!(find_project_settings(&sub, &settings_file).is_none());
        assert_eq!(
            find_project_settings(&sub, &dir.path().join("other.json")),
            Some(settings_file)
        );
    }
}
