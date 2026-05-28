//! Settings configuration types for the Ante extensibility system.
//!
//! Defines the deserialization shapes for `~/.ante/settings.json`
//! and `.claude/settings.json` (when Claude Code compat is enabled).

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::event::EventType;

// ─── Top-level settings ─────────────────────────────────────────────────────

/// Root Ante settings configuration.
///
/// Loaded from `~/.ante/settings.json` by default.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Extensibility hooks configuration.
    #[serde(default)]
    pub hooks: HooksConfig,

    /// MCP server registry.
    #[serde(default)]
    pub mcp_servers: Vec<MCPServerConfig>,

    /// Sub-agent definitions directory.
    #[serde(default)]
    pub agents: AgentsConfig,

    /// Memory / knowledge base configuration.
    #[serde(default)]
    pub memory: MemoryConfig,

    /// Model pool for dynamic model routing.
    #[serde(default)]
    pub model_pool: Vec<ModelPoolEntry>,

    /// Context budget limits.
    #[serde(default)]
    pub context_budget: ContextBudget,

    /// Claude Code compatibility flags.
    #[serde(default)]
    pub claude_compat: ClaudeCompatFlags,

    /// Sensitive operations requiring approval.
    #[serde(default)]
    pub sensitive_tools: Vec<String>,

    /// Whether to enable the full extensibility system.
    #[serde(default = "default_true")]
    pub extensibility_enabled: bool,

    /// Custom settings directory path (overrides `~/.ante`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ante_dir: Option<PathBuf>,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            extensibility_enabled: true,
            hooks: HooksConfig::default(),
            mcp_servers: Vec::new(),
            agents: AgentsConfig::default(),
            memory: MemoryConfig::default(),
            model_pool: Vec::new(),
            context_budget: ContextBudget::default(),
            claude_compat: ClaudeCompatFlags::default(),
            sensitive_tools: vec!["Bash".into(), "Write".into()],
            ante_dir: None,
        }
    }
}

// ─── Hooks ──────────────────────────────────────────────────────────────────

/// Hooks configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksConfig {
    /// Ordered list of hook match rules.
    #[serde(default)]
    pub rules: Vec<HookMatchRule>,

    /// Maximum hook nesting depth before a hook is skipped.
    #[serde(default = "default_hook_max_depth")]
    pub max_depth: u32,

    /// Default timeout in milliseconds for hook execution.
    #[serde(default = "default_hook_timeout_ms")]
    pub default_timeout_ms: u64,
}

fn default_hook_max_depth() -> u32 {
    5
}

fn default_hook_timeout_ms() -> u64 {
    30_000
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self { rules: Vec::new(), max_depth: default_hook_max_depth(), default_timeout_ms: default_hook_timeout_ms() }
    }
}

/// A rule that matches events to hook definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookMatchRule {
    /// Which event types this rule matches.
    pub event_types: Vec<EventType>,

    /// Optional regex pattern to match against the tool name.
    /// If absent, matches all tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name_pattern: Option<String>,

    /// The hooks to execute when this rule matches.
    pub hooks: Vec<HookDefinition>,
}

/// A single hook definition — can be a command, LLM prompt, MCP tool call,
/// or sub-agent dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HookDefinition {
    /// Execute a shell command or script.
    Command {
        /// Shell command to run.
        command: String,
        /// Arguments to pass to the command.
        #[serde(default)]
        args: Vec<String>,
        /// Override the default timeout for this hook (ms).
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Invoke an LLM with a prompt template.
    Prompt {
        /// The prompt template string. Uses `{event_json}` and `{event_type}`
        /// as template variables.
        prompt: String,
        /// The model to use for this hook.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Call an MCP server tool.
    McpTool {
        /// Name of the MCP server.
        server: String,
        /// Name of the tool to call.
        tool: String,
        /// Static arguments to pass to the tool.
        #[serde(default)]
        args: HashMap<String, Value>,
    },
    /// Dispatch a sub-agent for further processing.
    SubAgent {
        /// Name of the sub-agent to invoke (as registered in the agent
        /// registry).
        agent_name: String,
        /// Task prompt template.  Supports `{event_json}` and
        /// `{event_type}` template variables.
        task: String,
    },
}

// ─── MCP servers ────────────────────────────────────────────────────────────

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MCPServerConfig {
    /// Display name for the MCP server.
    pub name: String,
    /// The command to start the server process.
    pub command: String,
    /// Arguments to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
    /// Whether to auto-start this server on session begin.
    #[serde(default = "default_true")]
    pub auto_start: bool,
    /// Maximum restart attempts on crash.
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    /// Tool lifecycle mode: "lazy", "eager", or "keep-alive".
    #[serde(default = "default_lifecycle")]
    pub lifecycle: String,
    /// Idle timeout in minutes before the server is shut down.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout: Option<u64>,
}

fn default_max_restarts() -> u32 {
    3
}

fn default_lifecycle() -> String {
    "lazy".into()
}

// ─── Agents (sub-agents) ────────────────────────────────────────────────────

/// Sub-agents configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsConfig {
    /// Directory to scan for sub-agent `.md` files.
    #[serde(default = "default_agents_dir")]
    pub directory: PathBuf,
    /// Maximum number of sub-agents that can run concurrently.
    #[serde(default = "default_max_concurrent_agents")]
    pub max_concurrent: u32,
}

fn default_agents_dir() -> PathBuf {
    PathBuf::from("~/.ante/agents")
}

fn default_max_concurrent_agents() -> u32 {
    4
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self { directory: default_agents_dir(), max_concurrent: default_max_concurrent_agents() }
    }
}

/// A sub-agent definition loaded from a Markdown file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
}

fn default_max_turns() -> u32 {
    25
}

// ─── Memory ─────────────────────────────────────────────────────────────────

/// Memory / knowledge base configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryConfig {
    /// Path to the SQLite database file.
    #[serde(default = "default_memory_path")]
    pub db_path: PathBuf,
    /// Maximum number of memory entries to inject into context.
    #[serde(default = "default_max_context_memories")]
    pub max_context_memories: usize,
    /// Whether to auto-index PostToolUse events.
    #[serde(default = "default_true")]
    pub auto_index: bool,
}

fn default_memory_path() -> PathBuf {
    PathBuf::from("~/.ante/memory.db")
}

fn default_max_context_memories() -> usize {
    10
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self { db_path: default_memory_path(), max_context_memories: default_max_context_memories(), auto_index: true }
    }
}

// ─── Model pool ─────────────────────────────────────────────────────────────

/// Entry in the model pool for dynamic routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPoolEntry {
    /// User-friendly name for this model entry.
    pub name: String,
    /// Provider name (e.g. "anthropic", "openai", "custom").
    pub provider: String,
    /// The model identifier string for the provider.
    pub model_id: String,
    /// Capability score 0-100 (higher = more capable).
    pub capability_score: u32,
    /// Cost per 1K input tokens in USD.
    pub cost_per_1k_input: f64,
    /// Cost per 1K output tokens in USD.
    pub cost_per_1k_output: f64,
    /// Optional context window limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u32>,
    /// Whether this model is currently available for selection.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional tags for classification (e.g. "fast", "reasoning").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

// ─── Context budget ─────────────────────────────────────────────────────────

/// Context budget limits for token and cost tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextBudget {
    /// Maximum total tokens (input + output) per session.
    #[serde(default = "default_token_budget")]
    pub max_tokens: u64,
    /// Maximum cost in USD per session.
    #[serde(default = "default_cost_budget")]
    pub max_cost_usd: f64,
    /// Percentage of budget that triggers a warning (0.0 - 1.0).
    #[serde(default = "default_warn_threshold")]
    pub warn_at: f64,
}

fn default_token_budget() -> u64 {
    1_000_000
}

fn default_cost_budget() -> f64 {
    0.50
}

fn default_warn_threshold() -> f64 {
    0.8
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self { max_tokens: default_token_budget(), max_cost_usd: default_cost_budget(), warn_at: default_warn_threshold() }
    }
}

// ─── Claude Code compatibility ──────────────────────────────────────────────

/// Flags that control Claude Code settings.json compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCompatFlags {
    /// If true, also read `.claude/settings.json` and merge its hooks.
    #[serde(default)]
    pub merge_claude_settings: bool,

    /// If true, translate Ante event names to Claude Code names
    /// when invoking hooks from `.claude/settings.json`.
    #[serde(default = "default_true")]
    pub translate_event_names: bool,

    /// If true, write `.claude/settings.json` when saving hook config
    /// (bidirectional sync).
    #[serde(default)]
    pub write_claude_settings: bool,

    /// Path to override the Claude Code settings file location.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_settings_path: Option<PathBuf>,
}

impl Default for ClaudeCompatFlags {
    fn default() -> Self {
        Self { merge_claude_settings: false, translate_event_names: true, write_claude_settings: false, claude_settings_path: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_serde() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings).expect("serialize");
        let back: Settings = serde_json::from_str(&json).expect("deserialize");
        assert!(back.extensibility_enabled);
        assert_eq!(back.sensitive_tools, vec!["Bash", "Write"]);
    }

    #[test]
    fn hook_rule_serde() {
        let rule = HookMatchRule {
            event_types: vec![EventType::PreToolUse],
            tool_name_pattern: Some("Bash".into()),
            hooks: vec![
                HookDefinition::Command {
                    command: "/bin/sh".into(),
                    args: vec!["-c".into(), "echo 'blocked'".into()],
                    timeout_ms: None,
                },
            ],
        };
        let json = serde_json::to_string_pretty(&rule).expect("serialize");
        let back: HookMatchRule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.event_types, vec![EventType::PreToolUse]);
        assert_eq!(back.tool_name_pattern.as_deref(), Some("Bash"));
    }

    #[test]
    fn mcp_server_config_serde() {
        let cfg = MCPServerConfig {
            name: "time".into(),
            command: "uvx".into(),
            args: vec!["mcp-server-time".into()],
            env: HashMap::new(),
            auto_start: true,
            max_restarts: 3,
            lifecycle: "lazy".into(),
            idle_timeout: None,
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: MCPServerConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.command, "uvx");
    }

    #[test]
    fn full_settings_from_json() {
        let json = r#"{
            "hooks": {
                "rules": [{
                    "eventTypes": ["pre_tool_use"],
                    "toolNamePattern": "Bash",
                    "hooks": [
                        { "type": "command", "command": "/bin/sh", "args": ["-c", "exit 1"] }
                    ]
                }],
                "maxDepth": 3,
                "defaultTimeoutMs": 5000
            },
            "sensitiveTools": ["Bash", "Write", "Deploy"]
        }"#;
        let settings: Settings = serde_json::from_str(json).expect("parse");
        assert_eq!(settings.hooks.rules.len(), 1);
        assert_eq!(settings.sensitive_tools.len(), 3);
        assert_eq!(settings.hooks.max_depth, 3);
    }
}
