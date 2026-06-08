//! First-run initialization for Ante extensibility features.
//!
//! On first run, creates `~/.ante/settings.json` with default hooks,
//! installs the blocklist hook script, and creates the agents directory.

use std::fs;
use std::path::PathBuf;

use ante_protocol_shape::event::EventType;
use ante_protocol_shape::settings::{
    AgentsConfig, ClaudeCompatFlags, ContextBudget, HookDefinition, HookMatchRule, HooksConfig,
    MemoryConfig, Settings,
};
use thiserror::Error;

/// Default blocklist hook script content (shipped with Ante).
const BLOCKLIST_SCRIPT: &[u8] = include_bytes!("hooks/blocklist.sh");

/// Pre-compact memory usage logger hook script (shipped with Ante).
const PRE_COMPACT_SCRIPT: &[u8] = include_bytes!("hooks/pre_compact.py");

/// Session-end summary logger hook script (shipped with Ante).
const SESSION_END_SCRIPT: &[u8] = include_bytes!("hooks/session_end.py");

const DEFAULT_AGENT: &str = r#"---
name: general-purpose
description: Handle general codebase inspection, documentation, debugging, and implementation tasks.
prompt: You are an Ante sub-agent. Work within the requested scope, prefer small verifiable changes, and report concrete findings, files touched, verification run, and residual risks.
tools: Read,Bash,Write
model: opencode/deepseek-v4-flash-free
max_turns: 8
---

Use the supervising agent's task as the source of truth. Avoid destructive file operations unless explicitly authorized.
"#;

#[derive(Debug, Error)]
pub enum InitError {
    #[error("Failed to create directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Failed to write file {path}: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Settings already exist at {0} — run --force to overwrite")]
    AlreadyExists(PathBuf),
}

/// Run first-time setup.
///
/// Creates `~/.ante/` directory, writes default `settings.json`
/// with the blocklist hook enabled, and installs the hook script.
///
/// Returns `Ok(true)` if setup was performed, `Ok(false)` if already set up.
pub fn first_run_setup(force: bool) -> Result<bool, InitError> {
    let ante_dir = default_ante_dir();

    let settings_path = ante_dir.join("settings.json");
    if settings_path.exists() && !force {
        create_dir_if_missing(&ante_dir)?;
        create_dir_if_missing(&ante_dir.join("hooks"))?;
        create_dir_if_missing(&ante_dir.join("agents"))?;
        install_default_agent(&ante_dir)?;
        return Ok(false);
    }

    // Create directory structure
    create_dir_if_missing(&ante_dir)?;
    create_dir_if_missing(&ante_dir.join("hooks"))?;
    create_dir_if_missing(&ante_dir.join("agents"))?;
    create_dir_if_missing(&ante_dir.join("run"))?;
    install_default_agent(&ante_dir)?;

    // ── Shared memory directory (wiki-memory compatible) ───────────────
    // All agents share the same ~/ai-wiki directory.  If the wiki-memory
    // project is present at ~/code/wiki-memory, symlink ~/ai-wiki → its
    // wiki/ directory so the dream agent and hooks are immediately
    // available.  Otherwise create a plain directory that still works.
    let ai_wiki = default_ai_wiki_dir();
    if !ai_wiki.exists() {
        // Try to symlink to wiki-memory repo if present
        let wiki_memory_path =
            default_ante_dir() // ~ -> ~/code/wiki-memory/wiki
                .parent() // ~
                .map(|p| p.join("code").join("wiki-memory").join("wiki"));
        let symlinked = if let Some(ref src) = wiki_memory_path {
            if src.exists() || src.parent().map(|p| p.exists()).unwrap_or(false) {
                if !src.exists() {
                    create_dir_if_missing(src)?;
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(src, &ai_wiki).is_ok()
                }
                #[cfg(not(unix))]
                {
                    let _ = src;
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !symlinked {
            create_dir_if_missing(&ai_wiki)?;
        }
    }
    // Ensure .meta subdirectory exists inside the wiki
    create_dir_if_missing(&ai_wiki.join(".meta"))?;

    // Write blocklist hook script
    let hook_path = ante_dir.join("hooks").join("block-danger.sh");
    fs::write(&hook_path, BLOCKLIST_SCRIPT).map_err(|source| InitError::WriteFile {
        path: hook_path.clone(),
        source,
    })?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&hook_path, perms).map_err(|source| InitError::WriteFile {
            path: hook_path.clone(),
            source,
        })?;
    }

    // Write pre_compact.py hook script
    let pre_compact_path = ante_dir.join("hooks").join("pre_compact.py");
    fs::write(&pre_compact_path, PRE_COMPACT_SCRIPT).map_err(|source| InitError::WriteFile {
        path: pre_compact_path.clone(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&pre_compact_path, perms).map_err(|source| InitError::WriteFile {
            path: pre_compact_path.clone(),
            source,
        })?;
    }

    // Write session_end.py hook script
    let session_end_path = ante_dir.join("hooks").join("session_end.py");
    fs::write(&session_end_path, SESSION_END_SCRIPT).map_err(|source| InitError::WriteFile {
        path: session_end_path.clone(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&session_end_path, perms).map_err(|source| InitError::WriteFile {
            path: session_end_path.clone(),
            source,
        })?;
    }

    // Write default settings with blocklist hook
    let settings = default_settings(&ante_dir);
    let json = serde_json::to_string_pretty(&settings).map_err(|source| InitError::WriteFile {
        path: settings_path.clone(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, source.to_string()),
    })?;

    fs::write(&settings_path, &json).map_err(|source| InitError::WriteFile {
        path: settings_path.clone(),
        source,
    })?;

    Ok(true)
}

fn install_default_agent(ante_dir: &PathBuf) -> Result<(), InitError> {
    let agent_path = ante_dir.join("agents").join("general-purpose.md");
    if agent_path.exists() {
        return Ok(());
    }
    fs::write(&agent_path, DEFAULT_AGENT).map_err(|source| InitError::WriteFile {
        path: agent_path,
        source,
    })
}

/// Returns default settings with the blocklist, pre_compact, and session_end hooks pre-registered.
fn default_settings(ante_dir: &PathBuf) -> Settings {
    let hook_path = ante_dir.join("hooks").join("block-danger.sh");
    let pre_compact_path = ante_dir.join("hooks").join("pre_compact.py");
    let session_end_path = ante_dir.join("hooks").join("session_end.py");

    Settings {
        extensibility_enabled: true,
        hooks: HooksConfig {
            max_depth: 3,
            default_timeout_ms: 30_000,
            rules: vec![
                // Blocklist hook: blocks dangerous Bash commands
                HookMatchRule {
                    event_types: vec![EventType::PreToolUse],
                    tool_name_pattern: Some("Bash*".into()),
                    hooks: vec![HookDefinition::Command {
                        command: hook_path.display().to_string(),
                        args: vec![],
                        timeout_ms: Some(5000),
                    }],
                },
                // Pre-compact hook: logs memory/program usage before compaction
                HookMatchRule {
                    event_types: vec![EventType::PreCompact],
                    tool_name_pattern: None,
                    hooks: vec![HookDefinition::Command {
                        command: "python3".into(),
                        args: vec![pre_compact_path.display().to_string()],
                        timeout_ms: Some(10_000),
                    }],
                },
                // Session-end hook: logs session summary on exit
                HookMatchRule {
                    event_types: vec![EventType::SessionEnd],
                    tool_name_pattern: None,
                    hooks: vec![HookDefinition::Command {
                        command: "python3".into(),
                        args: vec![session_end_path.display().to_string()],
                        timeout_ms: Some(10_000),
                    }],
                },
            ],
        },
        sensitive_tools: vec!["Bash".into(), "Write".into(), "Execute".into()],
        mcp_servers: Vec::new(),
        agents: AgentsConfig {
            directory: ante_dir.join("agents"),
            max_concurrent: 4,
        },
        memory: MemoryConfig {
            db_path: default_memory_db_path(),
            max_context_memories: 20,
            auto_index: true,
        },
        model_pool: Vec::new(),
        context_budget: ContextBudget {
            max_tokens: 200_000,
            max_cost_usd: 1.0,
            warn_at: 0.8,
        },
        claude_compat: ClaudeCompatFlags {
            merge_claude_settings: true,
            translate_event_names: true,
            write_claude_settings: false,
            claude_settings_path: None,
        },
        ante_dir: Some(ante_dir.clone()),
    }
}

fn default_ante_dir() -> PathBuf {
    if let Ok(config) = std::env::var("XDG_CONFIG_HOME") {
        let candidate = PathBuf::from(config).join("ante");
        if candidate.exists() {
            return candidate;
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".ante");
    }
    PathBuf::from(".ante")
}

/// Returns the shared memory / wiki directory (`~/ai-wiki`).
///
/// This is the canonical location for all agent memory data.  It may be a
/// symlink to `wiki-memory/wiki/` when the wiki-memory project is
/// installed, or a plain directory otherwise.
fn default_ai_wiki_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join("ai-wiki")
    } else {
        PathBuf::from("./ai-wiki")
    }
}

fn default_memory_db_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let wiki_memory = PathBuf::from(&home).join("code").join("wiki-memory");
        if wiki_memory.exists() {
            return wiki_memory
                .join("wiki")
                .join(".meta")
                .join("ante-memory.db");
        }
        return PathBuf::from(home)
            .join("ai-wiki")
            .join(".meta")
            .join("ante-memory.db");
    }
    PathBuf::from("./ai-wiki/.meta/ante-memory.db")
}

fn create_dir_if_missing(path: &PathBuf) -> Result<(), InitError> {
    if !path.exists() {
        fs::create_dir_all(path).map_err(|source| InitError::CreateDir {
            path: path.clone(),
            source,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Tests that set HOME must use this lock to avoid race conditions
    // from parallel test execution sharing the process env.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn first_run_creates_and_forces_settings() {
        let _lock = HOME_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().expect("temp dir");
        unsafe { std::env::set_var("HOME", tmp.path().to_str().unwrap()) };

        // First run should set up
        let result = first_run_setup(false);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Verify files and hook scripts exist
        let ante_dir = tmp.path().join(".ante");
        assert!(ante_dir.join("settings.json").exists());
        assert!(ante_dir.join("hooks").join("block-danger.sh").exists());
        assert!(ante_dir.join("hooks").join("pre_compact.py").exists());
        assert!(ante_dir.join("hooks").join("session_end.py").exists());
        assert!(ante_dir.join("agents").exists());
        assert!(ante_dir.join("run").exists());

        // Shared memory directory (~/ai-wiki) should be created
        let ai_wiki = tmp.path().join("ai-wiki");
        assert!(ai_wiki.exists());
        assert!(ai_wiki.join(".meta").exists());

        // Second run with no force should detect existing setup
        let result2 = first_run_setup(false);
        assert!(result2.is_ok());
        assert!(!result2.unwrap());

        // Forced overwrite should re-setup
        let result3 = first_run_setup(true);
        assert!(result3.is_ok());
        assert!(result3.unwrap());
    }

    #[test]
    fn default_settings_has_all_hooks() {
        let dir = PathBuf::from("/home/user/.ante");
        let settings = default_settings(&dir);
        assert_eq!(settings.hooks.rules.len(), 3);
        assert_eq!(settings.sensitive_tools.len(), 3);

        // Rule 0: Blocklist hook (PreToolUse on Bash*)
        assert_eq!(
            settings.hooks.rules[0].event_types[0],
            EventType::PreToolUse
        );
        assert_eq!(
            settings.hooks.rules[0].tool_name_pattern,
            Some("Bash*".into())
        );

        // Rule 1: Pre-compact hook
        assert_eq!(
            settings.hooks.rules[1].event_types[0],
            EventType::PreCompact
        );
        assert!(settings.hooks.rules[1].tool_name_pattern.is_none());

        // Rule 2: Session-end hook
        assert_eq!(
            settings.hooks.rules[2].event_types[0],
            EventType::SessionEnd
        );
        assert!(settings.hooks.rules[2].tool_name_pattern.is_none());
    }
}
