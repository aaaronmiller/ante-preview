//! Ante — extensible agent runtime wrapping Claude Code.
//!
//! Modes:
//!   ante                 — Interactive REPL (default; session recording always on)
//!   ante --continue      — REPL, recovering the most recent session for this directory
//!   ante --resume <id>   — REPL, recovering a specific session
//!   ante query <prompt>  — One-shot query with full Ante tooling
//!   ante init            — First-run setup
//!   ante memory <cmd>    — Direct memory operations
//!   ante todo <cmd>      — Direct todo list operations
//!   ante agents <cmd>    — Sub-agent management
//!   ante diagram <file>  — Render Mermaid file to ASCII

mod mcp_server;
mod status;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use status::{StatusBar, render_banner};

use agent_sdk::agents::loader::AgentRegistry;
use agent_sdk::budget::{BudgetConfig, BudgetTracker};
use agent_sdk::claude::{
    AssistantMessage, Claude, ClaudeMessage, ClaudeOptions, ContentBlock, ControlRequestMessage,
    ControlResponseMessage, ResultMessage, StreamEventMessage, SystemMessage, UserMessage,
};
use agent_sdk::event::EventBus;
use agent_sdk::hitl::{ApprovalDecision, ApprovalManager, HitlMode, RiskLevel};
use agent_sdk::hooks::registry::HookRegistry;
use agent_sdk::init::first_run_setup;
use agent_sdk::mcp::registry::{McpServerConfigEntry, McpToolRegistry};
use agent_sdk::memory::server::MemoryServer;
use agent_sdk::memory::store::MemoryStore;
use agent_sdk::router::ModelRouter;
use agent_sdk::sessions::SessionManager;
use agent_sdk::settings::load_settings;
use agent_sdk::ui::diagram::render;
use agent_sdk::ui::todo::TodoList;
use ante_protocol_shape::payload::RiskLevel as ProtocolRiskLevel;
use ante_protocol_shape::settings::Settings;
use ante_protocol_shape::{
    BasePayload, CompactPayload, EventPayload, Id, PermissionRequestPayload, SessionEndPayload,
    SessionStartPayload, UserPromptPayload,
};
use clap::{Parser, Subcommand};

// ─── CLI ────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "ante", version, about = "Ante — extensible agent runtime")]
struct Cli {
    // ── Default mode: interactive REPL (ante ─ no subcommand) ─────────
    /// Continue from the most recent session in this directory
    #[arg(short, long)]
    continue_session: bool,

    /// Resume a specific session by ID (REPL mode)
    #[arg(long)]
    resume: Option<String>,

    /// Model override (e.g. claude-sonnet-4-5)
    #[arg(long)]
    model: Option<String>,

    /// Skip memory context injection
    #[arg(long)]
    no_memory: bool,

    /// Skip HITL approval system
    #[arg(long)]
    no_hitl: bool,

    /// HITL approval mode (per-request, batch-risk-threshold, approve-all)
    #[arg(long)]
    hitl_mode: Option<String>,

    /// Risk threshold for auto-approval (safe, low, medium, high, critical)
    #[arg(long)]
    risk_threshold: Option<String>,

    /// Disable model routing (use default model)
    #[arg(long)]
    no_router: bool,

    /// Path to Claude CLI binary
    #[arg(long)]
    cli_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive REPL (default — just run `ante`)
    Repl,

    /// One-shot query
    Query {
        /// Prompt text
        prompt: Vec<String>,

        /// Model override (e.g. claude-sonnet-4-5)
        #[arg(long)]
        model: Option<String>,

        /// Skip memory context injection
        #[arg(long)]
        no_memory: bool,

        /// Skip HITL approval system
        #[arg(long)]
        no_hitl: bool,

        /// HITL approval mode (per-request, batch-risk-threshold, approve-all)
        #[arg(long)]
        hitl_mode: Option<String>,

        /// Risk threshold for auto-approval (safe, low, medium, high, critical)
        #[arg(long)]
        risk_threshold: Option<String>,

        /// Disable model routing (use default model)
        #[arg(long)]
        no_router: bool,
    },

    /// First-run setup
    Init {
        /// Force re-initialization
        #[arg(long, short)]
        force: bool,
    },

    /// Direct memory operations
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },

    /// Direct todo list operations
    Todo {
        #[command(subcommand)]
        command: TodoCommands,
    },

    /// Sub-agent management
    Agents {
        #[command(subcommand)]
        command: AgentsCommands,
    },

    /// Session management (list, show, resume)
    Sessions {
        #[command(subcommand)]
        command: SessionCommands,
    },

    /// Check local production-readiness prerequisites
    Doctor,

    /// Render Mermaid diagram to ASCII
    Diagram {
        /// Path to Mermaid file or inline source
        source: Vec<String>,
    },

    /// Internal MCP server (spawned as child process by main agent)
    #[doc(hidden)]
    InternalMcpServer {
        /// Stub args placeholder
        #[arg(hide = true)]
        stub: Vec<String>,
    },
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Add a memory
    Add {
        content: String,
        #[arg(long, default_value = "")]
        tags: String,
        #[arg(long, default_value = "default")]
        project: String,
    },
    /// Search memories
    Search { query: String },
    /// Get context memories for project
    Context {
        #[arg(default_value = "default")]
        project: String,
        #[arg(long, default_value = "10")]
        max: usize,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// List all recorded sessions
    List {
        /// Project directory to filter by
        #[arg(long)]
        project: Option<String>,
    },
    /// Show details and messages from a session
    Show {
        /// Session ID (UUID)
        id: String,
        /// Number of recent messages to show
        #[arg(long, default_value = "10")]
        messages: usize,
    },
    /// Resume a session (inject its context into a new REPL)
    Resume {
        /// Session ID (UUID)
        id: String,
    },
}

#[derive(Subcommand)]
enum TodoCommands {
    /// Add a todo
    Add { text: Vec<String> },
    /// List all todos
    List,
    /// Mark todo done
    Done { id: usize },
    /// Clear all done todos
    Clear,
}

#[derive(Subcommand)]
enum AgentsCommands {
    /// List available sub-agents
    List {
        /// Override the agent directory
        #[arg(long)]
        agent_dir: Option<PathBuf>,
    },
    /// Match a task to the best available sub-agent without executing it
    Match {
        /// Override the agent directory
        #[arg(long)]
        agent_dir: Option<PathBuf>,
        /// Task text
        task: Vec<String>,
    },
    /// Run a task through the best matching sub-agent
    Run {
        /// Backend runner to use: opencode or dry-run
        #[arg(long, default_value = "opencode")]
        backend: String,
        /// Override the model. OpenCode format is provider/model.
        #[arg(long)]
        model: Option<String>,
        /// Override the agent directory
        #[arg(long)]
        agent_dir: Option<PathBuf>,
        /// Directory where the backend should run
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Write execution transcript and metadata to this file
        #[arg(long)]
        output: Option<PathBuf>,
        /// Show selected agent and rendered prompt without executing
        #[arg(long)]
        dry_run: bool,
        /// Add read-only instructions to the sub-agent prompt
        #[arg(long)]
        read_only: bool,
        /// Pass OpenCode's dangerously-skip-permissions flag
        #[arg(long)]
        skip_permissions: bool,
        /// Task text
        task: Vec<String>,
    },
}

// ─── Local Budget Tracking ──────────────────────────────────────────────────

/// Runtime budget snapshot we can read back for /budget display.
#[derive(Default)]
struct BudgetSnapshot {
    input_tokens: u64,
    output_tokens: u64,
    total_cost: f64,
    max_cost: f64,
    max_tokens: u64,
}

// ─── Agent Runtime ──────────────────────────────────────────────────────────

/// Runtime context shared across session execution.
#[allow(dead_code)]
struct AgentContext {
    settings: Settings,
    event_bus: Option<EventBus>,
    mcp_registry: Option<McpToolRegistry>,
    memory: Option<MemoryStore>,
    memory_server: Option<MemoryServer>,
    todo: Option<TodoList>,
    router: Option<ModelRouter>,
    approval: Option<ApprovalManager>,
    budget: BudgetTracker,
    budget_snapshot: BudgetSnapshot,
    ante_dir: PathBuf,
    status_bar: StatusBar,
    sessions: Option<SessionManager>,
}

impl AgentContext {
    /// Initialize all components from settings.
    fn initialize(settings: Settings) -> Self {
        let ante_dir = settings.ante_dir.clone().unwrap_or_else(|| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".ante"))
                .unwrap_or_else(|_| PathBuf::from(".ante"))
        });

        // ── Event bus + hooks ────────────────────────────────────────────
        let hook_registry = HookRegistry::new(settings.hooks.rules.clone());
        let event_bus = EventBus::new(hook_registry);

        // ── Budget tracker ───────────────────────────────────────────────
        let max_tokens = settings.context_budget.max_tokens;
        let max_cost = settings.context_budget.max_cost_usd;
        let budget = BudgetTracker::new(BudgetConfig {
            max_context_tokens: max_tokens,
            max_cost_usd: max_cost,
            warn_at: settings.context_budget.warn_at,
        });

        let budget_snapshot = BudgetSnapshot {
            max_cost,
            max_tokens,
            ..Default::default()
        };

        // ── Memory store ─────────────────────────────────────────────────
        let memory_db_path = expand_tilde_path(settings.memory.db_path.clone());
        let memory =
            MemoryStore::open(memory_db_path.clone(), settings.memory.max_context_memories).ok();

        let memory_server = memory
            .as_ref()
            .map(|_| {
                MemoryServer::open(memory_db_path.clone(), settings.memory.max_context_memories)
            })
            .and_then(|r| r.ok());

        // ── Todo list ────────────────────────────────────────────────────
        let todo = TodoList::open(ante_dir.join("todo.json")).ok();

        // ── Model router ─────────────────────────────────────────────────
        // Convert protocol-shape ModelPoolEntry to agent-sdk ModelPoolEntry
        let router_entries: Vec<agent_sdk::router::ModelPoolEntry> = settings
            .model_pool
            .iter()
            .map(|e| agent_sdk::router::ModelPoolEntry {
                model: e.model_id.clone(),
                capability: (e.capability_score / 10).min(10) as u8,
                cost_per_1k_input: e.cost_per_1k_input,
                cost_per_1k_output: e.cost_per_1k_output,
                max_context: e.context_limit.unwrap_or(200_000),
            })
            .collect();

        let router = if router_entries.is_empty() {
            None
        } else {
            Some(ModelRouter::new(router_entries))
        };

        // ── HITL Approval ────────────────────────────────────────────────
        let approval = Some(ApprovalManager::new());

        // ── Status bar ───────────────────────────────────────────────────
        let status_bar = StatusBar::new(None);

        // ── Session manager ──────────────────────────────────────────────
        let sessions_root = ante_dir.join("sessions");
        let sessions = Some(SessionManager::new(sessions_root));

        AgentContext {
            settings,
            event_bus: Some(event_bus),
            mcp_registry: None, // Connected lazily
            memory,
            memory_server,
            todo,
            router,
            approval,
            budget,
            budget_snapshot,
            ante_dir,
            status_bar,
            sessions,
        }
    }

    /// Connect MCP servers from settings and register internal tools.
    async fn connect_mcp_servers(&mut self) {
        let mut registry = McpToolRegistry::new();

        // ── Internal Ante tools server (diagram + todo) ──────────────────
        let internal_entry = McpServerConfigEntry {
            name: "ante-tools".to_string(),
            command: std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "ante".to_string()),
            args: vec!["internal-mcp-server".to_string()],
            auto_start: true,
        };
        match registry.register_server(internal_entry).await {
            Ok(_) => {
                eprintln!("[ante] Internal tools server connected");
            }
            Err(e) => {
                eprintln!("[ante] warning: internal tools server failed: {e}");
            }
        }

        // ── External MCP servers from settings ───────────────────────────
        for server_config in &self.settings.mcp_servers {
            if !server_config.auto_start {
                continue;
            }
            let entry = McpServerConfigEntry {
                name: server_config.name.clone(),
                command: server_config.command.clone(),
                args: server_config.args.clone(),
                auto_start: server_config.auto_start,
            };
            match registry.register_server(entry).await {
                Ok(_) => {
                    eprintln!("[ante] MCP server connected: {}", server_config.name);
                }
                Err(e) => {
                    eprintln!(
                        "[ante] warning: MCP server '{}' failed to connect: {e}",
                        server_config.name
                    );
                }
            }
        }
        self.mcp_registry = Some(registry);
    }

    /// Print the startup banner to stderr with feature stats.
    fn show_banner(&self) {
        let model = None; // Will be updated after Claude connects
        let memory_count = self
            .memory
            .as_ref()
            .map(|m| m.search("").len())
            .unwrap_or(0);
        let agent_count = match agent_sdk::agents::loader::AgentRegistry::load(&expand_tilde_path(
            self.settings.agents.directory.clone(),
        )) {
            Ok(reg) => reg.count(),
            Err(_) => 0,
        };
        eprint!(
            "{}",
            render_banner(
                env!("CARGO_PKG_VERSION"),
                model,
                self.settings.mcp_servers.len(),
                agent_count,
                memory_count,
            )
        );
    }

    /// Render and print the current status bar to stderr.
    /// Uses two-line detailed view for color terminals, compact for no-color.
    fn print_status(&self) {
        if self.status_bar.is_color() {
            let (r1, r2) = self.status_bar.render_detailed();
            eprint!("\r{}      \n{}", r1, r2);
        } else {
            let line = self.status_bar.render();
            eprint!("\r{}", line);
        }
    }

    /// Get memory context for a project as a formatted string.
    fn get_memory_context(&self, project: &str) -> Option<String> {
        let store = self.memory.as_ref()?;
        let entries = store.get_context(project, 10);
        if entries.is_empty() {
            return None;
        }
        let mut ctx = String::from("\n[Memory Context — from previous sessions]\n");
        for entry in &entries {
            let ts = &entry.timestamp[..8.min(entry.timestamp.len())];
            ctx.push_str(&format!("  [{ts}] {}\n", entry.content));
        }
        ctx.push_str("[/Memory Context]\n");
        Some(ctx)
    }

    /// Store a memory entry automatically after a session turn.
    fn auto_store_memory(&mut self, content: String, project: &str) {
        if let Some(store) = self.memory.as_mut() {
            // Auto-tag based on content keywords
            let tags = if content.contains("config")
                || content.contains("port")
                || content.contains("env")
            {
                "config"
            } else if content.contains("api")
                || content.contains("token")
                || content.contains("key")
            {
                "api"
            } else if content.contains("bug")
                || content.contains("fix")
                || content.contains("issue")
            {
                "bug"
            } else {
                "general"
            };
            if let Err(e) = store.add(content, tags.into(), project.into()) {
                eprintln!("[ante] warning: failed to store memory: {e}");
            }
        }
    }

    /// Classify a tool and check HITL approval.
    async fn check_hitl(&self, tool_name: &str, input: &serde_json::Value) -> Result<(), String> {
        let Some(ref approval) = self.approval else {
            return Ok(());
        };

        let input_str = match input {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        // classify() returns RiskLevel directly
        let risk: RiskLevel = approval.classify(tool_name, &input_str);

        // Safe/Low — no approval needed
        if !risk.requires_approval() {
            return Ok(());
        }

        eprintln!("\n⚠️  Tool requires approval: {tool_name} ({risk:?} risk)");
        eprintln!("  Input: {input}");

        // Prompt user
        print!("  Approve? [y/N/s] (y=yes, N=no, s=skip/always): ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut buf = String::new();
        io::stdin().read_line(&mut buf).map_err(|e| e.to_string())?;
        let buf = buf.trim().to_lowercase();

        match parse_approval_input(&buf) {
            ApprovalDecision::Approved => {
                eprintln!("  ✅ Approved");
                Ok(())
            }
            ApprovalDecision::Denied => {
                eprintln!("  ❌ Denied");
                Err("Action denied by user".to_string())
            }
            ApprovalDecision::TimedOut => {
                eprintln!("  ⏱️  Timed out");
                Err("Approval timed out".to_string())
            }
            ApprovalDecision::Modify(_) => {
                eprintln!("  ✅ Approved (with modifications)");
                Ok(())
            }
        }
    }
}

/// Parse user input into an approval decision.
fn parse_approval_input(input: &str) -> ApprovalDecision {
    match input {
        "y" | "yes" | "approve" | "allow" | "s" | "skip" | "always" => ApprovalDecision::Approved,
        _ => ApprovalDecision::Denied,
    }
}

// ─── Event Payload Helpers ──────────────────────────────────────────────────

/// Build a reusable base payload for events.
fn base_payload(cwd: &std::path::Path) -> BasePayload {
    BasePayload::new(cwd.to_path_buf(), "0.2.0".into())
}

/// Build a session-start payload.
fn session_start_payload(base: &BasePayload) -> SessionStartPayload {
    let cwd = std::env::current_dir().unwrap_or_default();
    SessionStartPayload {
        base: base.clone(),
        session_id: Id::ses(),
        model: String::new(),
        provider: String::new(),
        project_dir: Some(cwd.clone()),
        project_name: cwd.file_name().map(|n| n.to_string_lossy().to_string()),
    }
}

/// Build a session-end payload.
fn session_end_payload(base: &BasePayload) -> SessionEndPayload {
    SessionEndPayload {
        base: base.clone(),
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cost_usd: 0.0,
        duration_secs: 0,
        reason: None,
    }
}

/// Build a compact payload from the current budget snapshot.
fn compact_payload(base: &BasePayload, ctx: &AgentContext) -> CompactPayload {
    CompactPayload {
        base: base.clone(),
        current_tokens: ctx.budget_snapshot.input_tokens + ctx.budget_snapshot.output_tokens,
        budget_tokens: ctx.budget_snapshot.max_tokens,
        current_cost_usd: ctx.budget_snapshot.total_cost,
        budget_cost_usd: Some(ctx.budget_snapshot.max_cost),
    }
}

/// Build a user-prompt payload.
fn user_prompt_payload(base: &BasePayload, prompt: &str) -> UserPromptPayload {
    UserPromptPayload {
        base: base.clone(),
        prompt: prompt.to_string(),
        model: String::new(),
        turn_count: 0,
    }
}

// ─── REPL Loop ──────────────────────────────────────────────────────────────

async fn run_repl_with_options(
    ctx: &mut AgentContext,
    cli_options: ClaudeOptions,
    continue_session: bool,
    resume_session_id: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Connect MCP servers
    ctx.connect_mcp_servers().await;

    let cwd = std::env::current_dir().unwrap_or_default();
    let bp = base_payload(&cwd);

    // Fire session start event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus
            .emit(&EventPayload::SessionStart(session_start_payload(&bp)))
            .await;
    }

    eprintln!("Connecting to Claude CLI...");
    let mut client = Claude::connect(cli_options).await?;
    let model_name = client
        .server_info()
        .and_then(|info| info.get("model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "claude".to_string());
    ctx.status_bar.set_model(&model_name);
    ctx.print_status();
    eprintln!("Connected. Type /help for commands.\n");

    // ── Start session logging ────────────────────────────────────────────
    if let Some(ref sessions) = ctx.sessions {
        let _ = sessions.start(&cwd, Some("anthropic"), Some(&model_name));
    }

    // ── Session recovery (opt-in: --continue or --resume) ────────────────
    let recovered_context = if let Some(ref sid) = resume_session_id {
        // Resume a specific session by ID
        if let Some(ref sessions) = ctx.sessions {
            match sessions.read_session(sid) {
                Ok(Some(lines)) => {
                    let messages: Vec<String> = lines
                        .iter()
                        .filter_map(|line| {
                            use agent_sdk::sessions::SessionLine;
                            match line {
                                SessionLine::Message(msg) => {
                                    let role = &msg.message.role;
                                    let content_str = match &msg.message.content {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Array(arr) => arr
                                            .iter()
                                            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                        _ => String::new(),
                                    };
                                    if content_str.trim().is_empty() {
                                        None
                                    } else {
                                        Some(format!("[{role}] {content_str}"))
                                    }
                                }
                                _ => None,
                            }
                        })
                        .collect();

                    if !messages.is_empty() {
                        eprintln!("[ante] Resumed session {sid}");
                        let ctx_str = messages.join("\n");
                        Some(format!(
                            "\n[Previous session {sid} context]\n{ctx_str}\n[/Previous session context]\n"
                        ))
                    } else {
                        None
                    }
                }
                Ok(None) => {
                    eprintln!("[ante] Session {sid} not found");
                    None
                }
                Err(e) => {
                    eprintln!("[ante] Warning: failed to read session {sid}: {e}");
                    None
                }
            }
        } else {
            None
        }
    } else if continue_session {
        // Auto-recover the latest session for this directory
        if let Some(ref sessions) = ctx.sessions {
            match sessions.recover_context(&cwd, 20) {
                Ok(Some((ctx_str, sid))) => {
                    eprintln!("[ante] Continuing from session {sid}");
                    Some(ctx_str)
                }
                Ok(None) => {
                    eprintln!("[ante] No previous session for this directory");
                    None
                }
                Err(e) => {
                    eprintln!("[ante] Warning: session recovery failed: {e}");
                    None
                }
            }
        } else {
            None
        }
    } else {
        // Default: fresh session
        None
    };

    let mut line = String::new();
    let mut first_turn = true;
    loop {
        print!("you> ");
        io::stdout().flush()?;

        line.clear();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Check for commands
        if input.starts_with('/') {
            let should_quit = handle_command(input, &mut client, ctx, &bp).await?;
            if should_quit {
                break;
            }
            continue;
        }

        // Fire PreUserPromptSubmit event
        if let Some(ref bus) = ctx.event_bus {
            let payload = EventPayload::PreUserPromptSubmit(user_prompt_payload(&bp, input));
            let result = bus.emit(&payload).await;
            if !result.decision.is_allowed() {
                eprintln!("[ante] Prompt blocked by hook: {:?}", result.hooks_executed);
                continue;
            }
        }

        // Record user message in session log
        if let Some(ref sessions) = ctx.sessions {
            let _ = sessions.record_user_message(input);
        }

        // Build prompt with context injection on first turn only
        let project = PathBuf::from(".")
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".to_string());

        let final_prompt = if first_turn {
            first_turn = false;
            if let Some(ref ctx_str) = recovered_context {
                format!("{ctx_str}\n\n{input}")
            } else if let Some(mem_ctx) = ctx.get_memory_context(&project) {
                format!("{mem_ctx}\n\n{input}")
            } else {
                input.to_string()
            }
        } else if let Some(mem_ctx) = ctx.get_memory_context(&project) {
            // Re-inject memory context each turn (it may change)
            format!("{mem_ctx}\n\n{input}")
        } else {
            input.to_string()
        };

        client.send_user_text(&final_prompt).await?;

        println!();
        stream_response(&mut client, ctx, &bp).await?;
        println!();

        // Fire PostUserPromptSubmit event
        if let Some(ref bus) = ctx.event_bus {
            let payload = EventPayload::PostUserPromptSubmit(user_prompt_payload(&bp, input));
            let _ = bus.emit(&payload).await;
        }
    }

    // ── End session logging ──────────────────────────────────────────────
    if let Some(ref sessions) = ctx.sessions {
        let total = ctx.budget_snapshot.input_tokens + ctx.budget_snapshot.output_tokens;
        let _ = sessions.end(total);
    }

    // Fire session end event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus
            .emit(&EventPayload::SessionEnd(session_end_payload(&bp)))
            .await;
    }

    // Disconnect MCP servers
    if let Some(ref mut registry) = ctx.mcp_registry {
        let tools: Vec<_> = registry.list_tools();
        for tool_id in tools {
            registry.disconnect(&tool_id.server).await;
        }
    }

    client.shutdown().await?;
    eprintln!("Session ended.");
    Ok(())
}

/// Stream Claude's response, handling control requests and tool interactions.
async fn stream_response(
    client: &mut Claude,
    ctx: &mut AgentContext,
    bp: &BasePayload,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let message = client.next_message().await?;

        match &message {
            ClaudeMessage::ControlRequest(control_request) => {
                if let Some(request_id) = control_request.request_id.as_deref() {
                    handle_control_request(client, control_request, request_id, ctx, bp).await?;
                }
            }
            ClaudeMessage::Assistant(assistant) => {
                render_assistant(assistant);

                // Record assistant response in session log
                let text_parts: Vec<&str> = assistant
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect();
                if !text_parts.is_empty() {
                    let content = serde_json::json!(text_parts);
                    if let Some(ref sessions) = ctx.sessions {
                        let model = assistant.model.as_deref();
                        let _ = sessions.record_assistant_message(content, model, None);
                    }
                }
            }
            ClaudeMessage::User(user) => {
                render_user(user);
            }
            ClaudeMessage::System(sys) => {
                render_system(sys);
            }
            ClaudeMessage::StreamEvent(event) => {
                render_stream_event(event);
            }
            ClaudeMessage::ControlResponse(response) => {
                render_control_response(response);
            }
            ClaudeMessage::Result(result) => {
                render_result(result);

                // Update budget tracking
                if let Some(usage) = &result.usage {
                    if let Some(input_tokens) = usage.get("input_tokens").and_then(|v| v.as_u64()) {
                        ctx.budget_snapshot.input_tokens += input_tokens;
                    }
                    if let Some(output_tokens) = usage.get("output_tokens").and_then(|v| v.as_u64())
                    {
                        ctx.budget_snapshot.output_tokens += output_tokens;
                    }
                }
                if let Some(cost) = result.total_cost_usd {
                    ctx.budget.add_cost(cost);
                    ctx.budget_snapshot.total_cost += cost;
                }

                // Update status bar with latest cumulative usage
                ctx.status_bar.set_usage(
                    ctx.budget_snapshot.input_tokens,
                    ctx.budget_snapshot.output_tokens,
                    ctx.budget_snapshot.total_cost,
                );
                ctx.status_bar.add_turn();
                ctx.print_status();

                // Check budget limits — emit PreCompact when near or over limits
                if let Err(e) = ctx.budget.check_limits() {
                    if let Some(ref bus) = ctx.event_bus {
                        let compact = compact_payload(bp, ctx);
                        let _ = bus.emit(&EventPayload::PreCompact(compact)).await;
                    }
                    eprintln!("\n[ante] ⚠️ Budget limit hit: {e}");
                } else if let Some(warn) = ctx.budget.warn_message() {
                    if let Some(ref bus) = ctx.event_bus {
                        let compact = compact_payload(bp, ctx);
                        let _ = bus.emit(&EventPayload::PreCompact(compact)).await;
                    }
                    eprintln!("\n[ante] ⚠️ {warn}");
                }

                // Auto-store memory for key information from this turn
                if let Some(ref result_val) = result.result {
                    if let Some(result_text) = result_val.as_str() {
                        let content = if result_text.len() > 500 {
                            format!("{}...", &result_text[..500])
                        } else {
                            result_text.to_string()
                        };
                        let project = PathBuf::from(".")
                            .canonicalize()
                            .ok()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .unwrap_or_else(|| "default".to_string());
                        ctx.auto_store_memory(content, &project);
                    }
                }

                // Record Result as assistant response in session log
                // (covers tool results and final text in the result payload)
                if let Some(ref result_val) = result.result {
                    if let Some(result_text) = result_val.as_str() {
                        if !result_text.trim().is_empty() {
                            let usage = result
                                .usage
                                .clone()
                                .map(|u| serde_json::to_value(u).unwrap_or_default());
                            let content =
                                serde_json::json!([{"type": "text", "text": result_text}]);
                            if let Some(ref sessions) = ctx.sessions {
                                // Use model name from status bar (set at connect time)
                                let model = ctx.status_bar.model();
                                let _ = sessions.record_assistant_message(content, model, usage);
                            }
                        }
                    }
                }

                // Update session token tracking
                if let Some(ref sessions) = ctx.sessions {
                    if let Some(usage) = &result.usage {
                        let total = usage
                            .get("input_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            + usage
                                .get("output_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                        sessions.add_tokens(total);
                    }
                }

                // Result means one turn completed
                return Ok(());
            }
            ClaudeMessage::Other(value) => {
                eprintln!("\n[other] {value}");
            }
        }
    }
}

/// Handle a control request from Claude Code.
async fn handle_control_request(
    client: &mut Claude,
    request: &ControlRequestMessage,
    request_id: &str,
    ctx: &mut AgentContext,
    bp: &BasePayload,
) -> Result<(), Box<dyn std::error::Error>> {
    match request.subtype.as_deref() {
        Some("tool_permission") | Some("allow_tool") | None => {
            // Extract tool info from the request
            let (tool_name, tool_input) = extract_tool_request(&request.request);

            // Fire PermissionRequest event
            if let Some(ref bus) = ctx.event_bus {
                if let Some(name) = &tool_name {
                    let payload = EventPayload::PermissionRequest(PermissionRequestPayload {
                        base: bp.clone(),
                        tool_name: name.clone(),
                        input: tool_input.clone().unwrap_or(serde_json::Value::Null),
                        risk_level: ProtocolRiskLevel::Medium,
                        message: format!("Tool '{}' requires permission", name),
                        can_modify: true,
                    });
                    let result = bus.emit(&payload).await;
                    if !result.decision.is_allowed() {
                        ctx.status_bar.track_tool_blocked();
                        ctx.status_bar.track_hook_blocked();
                        client
                            .respond_control_request_error(
                                request_id,
                                &format!("Blocked by hook: {:?}", result.hooks_executed),
                            )
                            .await?;
                        return Ok(());
                    }
                }
            }

            // Check HITL approval
            if let (Some(name), Some(input)) = (&tool_name, &tool_input) {
                if let Err(reason) = ctx.check_hitl(name, input).await {
                    ctx.status_bar.track_tool_blocked();
                    ctx.status_bar.track_hitl_denied();
                    client
                        .respond_control_request_error(
                            request_id,
                            &format!("Denied by HITL: {reason}"),
                        )
                        .await?;
                    return Ok(());
                }
            }

            // Default: allow if HITL passes
            ctx.status_bar.track_tool_ok();
            ctx.status_bar.track_hook_fired();
            ctx.status_bar.track_hitl_approved();
            client
                .respond_control_request_error(request_id, "HITL approved — tool permitted")
                .await?;
        }
        Some("initialize") => {
            // Claude already handles init during connect; pass through
            client
                .respond_control_request_error(request_id, "init already handled")
                .await?;
        }
        Some(other) => {
            // Unknown control request — reject with error
            client
                .respond_control_request_error(
                    request_id,
                    &format!("unsupported control request type: {other}"),
                )
                .await?;
        }
    }

    Ok(())
}

/// Extract tool name and input from a control request payload.
fn extract_tool_request(
    request: &Option<serde_json::Value>,
) -> (Option<String>, Option<serde_json::Value>) {
    let req = match request {
        Some(r) => r,
        None => return (None, None),
    };

    let tool_name = req
        .get("tool")
        .or_else(|| req.get("tool_name"))
        .or_else(|| req.get("name"))
        .and_then(|v| v.as_str().map(String::from));

    let tool_input = req
        .get("input")
        .or_else(|| req.get("arguments"))
        .or_else(|| req.get("args"))
        .cloned();

    (tool_name, tool_input)
}

// ─── CLI Command Handlers ───────────────────────────────────────────────────

async fn handle_command(
    input: &str,
    client: &mut Claude,
    ctx: &mut AgentContext,
    _bp: &BasePayload,
) -> Result<bool, Box<dyn std::error::Error>> {
    let parts: Vec<&str> = input.splitn(2, ' ').collect();
    let cmd = parts[0];
    let arg = parts.get(1).map(|v| v.trim()).unwrap_or("");

    match cmd {
        "/exit" | "/quit" => return Ok(true),
        "/help" => {
            eprintln!("Commands:");
            eprintln!("  /exit, /quit        End session");
            eprintln!("  /model <name>       Switch model");
            eprintln!("  /memory <text>      Store a memory from this session");
            eprintln!("  /mem-search <q>     Search memories");
            eprintln!("  /todo <text>        Add a todo");
            eprintln!("  /todos              List todos");
            eprintln!("  /done <id>          Mark todo done");
            eprintln!("  /diagram <mermaid>  Render a Mermaid diagram");
            eprintln!("  /budget             Show budget usage");
            eprintln!("  /interrupt          Interrupt current generation");
            eprintln!("  /info               Show Claude session info");
            eprintln!("  /help               Show this help");
        }
        "/model" => {
            if arg.is_empty() {
                eprintln!("Usage: /model <model-name>");
            } else {
                let _ = client.set_model(arg).await?;
                eprintln!("Model switched to: {arg}");
            }
        }
        "/memory" => {
            if arg.is_empty() {
                eprintln!("Usage: /memory <content to remember>");
            } else {
                let project = PathBuf::from(".")
                    .canonicalize()
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                    .unwrap_or_else(|| "default".to_string());
                ctx.auto_store_memory(arg.to_string(), &project);
                eprintln!("✅ Memory stored.");
            }
        }
        "/mem-search" => {
            if arg.is_empty() {
                eprintln!("Usage: /mem-search <query>");
            } else if let Some(ref store) = ctx.memory {
                let results = store.search(arg);
                if results.is_empty() {
                    eprintln!("No memories found for: {arg}");
                } else {
                    eprintln!("Memories matching '{arg}':");
                    for entry in results {
                        let ts = &entry.timestamp[..8.min(entry.timestamp.len())];
                        eprintln!(
                            "  [{ts}] [{}] {} (project: {})",
                            entry.tags, entry.content, entry.project
                        );
                    }
                }
            } else {
                eprintln!("Memory store not available.");
            }
        }
        "/todo" => {
            if arg.is_empty() {
                eprintln!("Usage: /todo <text>");
            } else if let Some(ref mut todos) = ctx.todo {
                match todos.add(arg) {
                    Ok(item) => eprintln!("✅ Todo #{}: {}", item.id, item.text),
                    Err(e) => eprintln!("Error: {e}"),
                }
            } else {
                eprintln!("Todo list not available.");
            }
        }
        "/todos" => {
            if let Some(ref todos) = ctx.todo {
                let items = todos.list();
                if items.is_empty() {
                    eprintln!("No todos.");
                } else {
                    eprintln!("Todos:");
                    for item in items {
                        let status = if item.done { "✓" } else { " " };
                        eprintln!("  [{status}] #{} {}", item.id, item.text);
                    }
                }
            } else {
                eprintln!("Todo list not available.");
            }
        }
        "/done" => {
            if let Ok(id) = arg.parse::<usize>() {
                if let Some(ref mut todos) = ctx.todo {
                    match todos.complete(id) {
                        Ok(item) => eprintln!("✅ Completed: {}", item.text),
                        Err(e) => eprintln!("Error: {e}"),
                    }
                }
            } else {
                eprintln!("Usage: /done <id>");
            }
        }
        "/diagram" => {
            if arg.is_empty() {
                eprintln!("Usage: /diagram <mermaid source>");
            } else {
                match render(arg) {
                    Ok(ascii) => println!("{ascii}"),
                    Err(e) => eprintln!("Diagram error: {e}"),
                }
            }
        }
        "/budget" => {
            let snap = &ctx.budget_snapshot;
            let total = snap.input_tokens + snap.output_tokens;
            eprintln!("Budget usage:");
            eprintln!("  Input tokens:  {}", snap.input_tokens);
            eprintln!("  Output tokens: {}", snap.output_tokens);
            eprintln!("  Total tokens:  {}", total);
            eprintln!("  Cost:          ${:.5}", snap.total_cost);
            eprintln!("  Max cost:      ${:.5}", snap.max_cost);
            eprintln!("  Max tokens:    {}", snap.max_tokens);
            eprintln!();
            // Check budget limits
            if ctx.budget.is_over_limit() {
                eprintln!("  ⚠️  Budget limit exceeded!");
            } else if let Some(warn) = ctx.budget.warn_message() {
                eprintln!("  ⚠️  {warn}");
            }
        }
        "/interrupt" => {
            let response = client.interrupt().await?;
            eprintln!("Interrupted: {response}");
        }
        "/info" => {
            eprintln!("Claude session active.");
        }
        _ => {
            eprintln!("Unknown command: {cmd}. Type /help for available commands.");
        }
    }

    Ok(false)
}

// ─── Rendering ──────────────────────────────────────────────────────────────

fn render_assistant(message: &AssistantMessage) {
    let model = message.model.as_deref();
    print_header("assistant", model);

    for block in &message.content {
        match block {
            ContentBlock::Text(text) => {
                for line in text.text.lines() {
                    println!("  {line}");
                }
            }
            ContentBlock::Thinking(thinking) => {
                println!("  [thinking]");
                for line in thinking.thinking.lines() {
                    println!("  {line}");
                }
            }
            ContentBlock::ToolUse(tool) => {
                println!("  [tool_use] {} (id={})", tool.name, tool.id);
                let pretty = serde_json::to_string_pretty(&tool.input)
                    .unwrap_or_else(|_| tool.input.to_string());
                for line in pretty.lines() {
                    println!("  {line}");
                }
            }
            ContentBlock::ToolResult(result) => {
                let tag = match result.is_error {
                    Some(true) => "tool_result:error",
                    _ => "tool_result",
                };
                println!("  [{tag}] (tool_use_id={})", result.tool_use_id);
                if let Some(content) = &result.content {
                    let pretty = serde_json::to_string_pretty(content)
                        .unwrap_or_else(|_| content.to_string());
                    for line in pretty.lines() {
                        println!("  {line}");
                    }
                }
            }
            ContentBlock::Other(value) => {
                println!("  [other block]");
                let pretty =
                    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
                for line in pretty.lines() {
                    println!("  {line}");
                }
            }
        }
    }

    if let Some(error) = &message.error {
        println!("  error: {error}");
    }
}

fn render_user(message: &UserMessage) {
    print_header("user", None);
    if let Some(text) = message.text() {
        for line in text.lines() {
            println!("  {line}");
        }
    } else {
        let pretty =
            serde_json::to_string_pretty(&message.raw).unwrap_or_else(|_| message.raw.to_string());
        for line in pretty.lines() {
            println!("  {line}");
        }
    }
}

fn render_system(message: &SystemMessage) {
    print_header("system", message.subtype.as_deref());
    let pretty =
        serde_json::to_string_pretty(&message.raw).unwrap_or_else(|_| message.raw.to_string());
    for line in pretty.lines() {
        println!("  {line}");
    }
}

fn render_stream_event(message: &StreamEventMessage) {
    print_header("stream_event", None);
    if let Some(event) = &message.event {
        let pretty = serde_json::to_string_pretty(event).unwrap_or_else(|_| event.to_string());
        for line in pretty.lines() {
            println!("  {line}");
        }
    }
}

fn render_control_response(message: &ControlResponseMessage) {
    print_header("control_response", message.subtype.as_deref());
    if let Some(error) = &message.error {
        println!("  error: {error}");
    }
    if let Some(response) = &message.response {
        let pretty =
            serde_json::to_string_pretty(response).unwrap_or_else(|_| response.to_string());
        for line in pretty.lines() {
            println!("  {line}");
        }
    }
}

fn render_result(message: &ResultMessage) {
    print_header("result", message.subtype.as_deref());
    let cost = message.total_cost_usd.unwrap_or(0.0);
    let duration_s = message.duration_ms.unwrap_or(0.0) / 1000.0;
    let api_s = message.duration_api_ms.unwrap_or(0.0) / 1000.0;
    let turns = message.num_turns.unwrap_or(0);
    println!("  turns: {turns}");
    println!("  cost:  ${cost:.4}");
    println!("  time:  {duration_s:.2}s (api {api_s:.2}s)");
    if let Some(session_id) = &message.session_id {
        println!("  session: {session_id}");
    }
    if let Some(usage) = &message.usage {
        println!("  usage:");
        let pretty = serde_json::to_string_pretty(usage).unwrap_or_else(|_| usage.to_string());
        for line in pretty.lines() {
            println!("  {line}");
        }
    }
    if let Some(result) = &message.result {
        match result {
            serde_json::Value::String(text) => {
                println!("  result:");
                for line in text.lines() {
                    println!("  {line}");
                }
            }
            other => {
                println!("  result:");
                let pretty =
                    serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string());
                for line in pretty.lines() {
                    println!("  {line}");
                }
            }
        }
    }
}

fn print_header(kind: &str, tag: Option<&str>) {
    println!();
    match tag {
        Some(tag) => println!("── {kind} ({tag}) ──"),
        None => println!("── {kind} ──"),
    }
}

// ─── Main Entry Point ───────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Default (no subcommand) = interactive REPL with always-on recording & recovery
    let subcommand = cli.command.unwrap_or(Commands::Repl);
    match subcommand {
        Commands::Init { force } => handle_init(force)?,
        Commands::Query {
            prompt,
            model,
            no_memory,
            no_hitl,
            hitl_mode,
            risk_threshold,
            no_router,
        } => {
            handle_query(
                prompt,
                model,
                no_memory,
                no_hitl,
                hitl_mode,
                risk_threshold,
                no_router,
            )
            .await?;
        }
        Commands::Repl => {
            handle_repl(
                cli.model,
                cli.no_memory,
                cli.no_hitl,
                cli.hitl_mode,
                cli.risk_threshold,
                cli.no_router,
                cli.cli_path,
                cli.continue_session,
                cli.resume,
            )
            .await?;
        }
        Commands::Sessions { command } => {
            handle_sessions(command)?;
        }
        Commands::Doctor => {
            handle_doctor()?;
        }
        Commands::Memory { command } => {
            handle_memory_direct(command)?;
        }
        Commands::Todo { command } => {
            handle_todo_direct(command)?;
        }
        Commands::Agents { command } => {
            handle_agents(command)?;
        }
        Commands::Diagram { source } => {
            handle_diagram(source)?;
        }
        Commands::InternalMcpServer { stub: _ } => {
            // Hidden subcommand: MCP server providing diagram + todo tools.
            // Spawned as a child process by the main Ante agent and
            // communicates via JSON-RPC 2.0 over stdio.
            mcp_server::run_mcp_server()?;
        }
    }

    Ok(())
}

fn handle_init(force: bool) -> Result<(), Box<dyn std::error::Error>> {
    match first_run_setup(force) {
        Ok(true) => eprintln!("✓ Ante initialized in ~/.ante/"),
        Ok(false) => eprintln!("Ante already initialized. Use --force to re-initialize."),
        Err(e) => eprintln!("Error: {e}"),
    }
    Ok(())
}

async fn handle_query(
    prompt: Vec<String>,
    model: Option<String>,
    no_memory: bool,
    no_hitl: bool,
    hitl_mode: Option<String>,
    risk_threshold: Option<String>,
    no_router: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let prompt_text = if prompt.is_empty() {
        eprintln!("Error: no prompt provided");
        return Ok(());
    } else {
        prompt.join(" ")
    };

    // Run first-run setup if needed
    let _ = first_run_setup(false);

    // Load settings
    let settings = match load_settings() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[ante] Using default settings");
            Settings::default()
        }
    };

    // Initialize context
    let mut ctx = AgentContext::initialize(settings);

    // Run MCP connection (best-effort)
    ctx.connect_mcp_servers().await;

    // Show startup banner
    ctx.show_banner();

    // Update status bar with MCP count, memory entries, and todos
    if let Some(ref reg) = ctx.mcp_registry {
        let tools: Vec<_> = reg.list_tools();
        let connected_count = tools
            .iter()
            .map(|t| &t.server)
            .collect::<std::collections::HashSet<_>>()
            .len();
        ctx.status_bar
            .set_mcp_servers(ctx.settings.mcp_servers.len(), connected_count);
    }
    if let Some(ref mem) = ctx.memory {
        ctx.status_bar.set_memory_entries(mem.search("").len());
    }
    if let Some(ref todos) = ctx.todo {
        let items = todos.list();
        let active = items.iter().filter(|i| !i.done).count() as u32;
        let done = items.iter().filter(|i| i.done).count() as u32;
        ctx.status_bar.set_todo_counts(active, done);
    }

    // Build Claude options
    let mut options = ClaudeOptions::default();

    // Model router
    let selected_model = if !no_router {
        if let Some(ref router) = ctx.router {
            match router.select(&prompt_text, 0) {
                Ok(decision) => {
                    eprintln!(
                        "[ante] Model router: {} ({})",
                        decision.selected_model, decision.reason
                    );
                    Some(decision.selected_model)
                }
                Err(_) => model.clone(),
            }
        } else {
            model.clone()
        }
    } else {
        model.clone()
    };

    if let Some(ref m) = selected_model {
        options.model = Some(m.clone());
    }

    // Apply HITL mode and risk threshold
    if let Some(ref mode_str) = hitl_mode {
        if let Some(mode) = HitlMode::from_str(mode_str) {
            if let Some(ref mut approval) = ctx.approval {
                *approval = std::mem::take(approval).with_mode(mode);
            }
        } else {
            eprintln!("[ante] Warning: unknown HITL mode '{mode_str}', using default");
        }
    }
    if let Some(ref threshold_str) = risk_threshold {
        if let Some(level) = RiskLevel::from_str(threshold_str) {
            if let Some(ref mut approval) = ctx.approval {
                *approval = std::mem::take(approval).with_risk_threshold(level);
            }
        } else {
            eprintln!("[ante] Warning: unknown risk threshold '{threshold_str}', using default");
        }
    }

    if no_hitl {
        ctx.approval = None;
    }

    // Memory context injection
    if !no_memory {
        let project = PathBuf::from(".")
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "default".to_string());

        if let Some(ctx_text) = ctx.get_memory_context(&project) {
            options.append_system_prompt = Some(ctx_text);
        }
    }

    eprintln!("Connecting to Claude...");
    let bp = base_payload(&std::env::current_dir().unwrap_or_default());
    let mut client = Claude::connect(options).await?;

    // Fire SessionStart event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus
            .emit(&EventPayload::SessionStart(session_start_payload(&bp)))
            .await;
    }

    // Fire PreUserPromptSubmit event
    let prompt_allowed = if let Some(ref bus) = ctx.event_bus {
        let payload = EventPayload::PreUserPromptSubmit(user_prompt_payload(&bp, &prompt_text));
        let result = bus.emit(&payload).await;
        if !result.decision.is_allowed() {
            eprintln!("[ante] Prompt blocked by hook: {:?}", result.hooks_executed);
            false
        } else {
            true
        }
    } else {
        true
    };

    if !prompt_allowed {
        client.shutdown().await?;
        return Ok(());
    }

    // ── Start session logging ────────────────────────────────────────────
    let cwd = std::env::current_dir().unwrap_or_default();
    if let Some(ref sessions) = ctx.sessions {
        let model_name = client
            .server_info()
            .and_then(|info| info.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("claude");
        let _ = sessions.start(&cwd, Some("anthropic"), Some(model_name));
        let _ = sessions.record_user_message(&prompt_text);
    }

    // Send query
    client.send_user_text(&prompt_text).await?;

    // Fire PostUserPromptSubmit event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus
            .emit(&EventPayload::PostUserPromptSubmit(user_prompt_payload(
                &bp,
                &prompt_text,
            )))
            .await;
    }

    // Stream response, handling control requests etc.
    stream_response(&mut client, &mut ctx, &bp).await?;

    // ── End session logging ──────────────────────────────────────────────
    if let Some(ref sessions) = ctx.sessions {
        let total = ctx.budget_snapshot.input_tokens + ctx.budget_snapshot.output_tokens;
        let _ = sessions.end(total);
    }

    // Fire SessionEnd event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus
            .emit(&EventPayload::SessionEnd(session_end_payload(&bp)))
            .await;
    }

    client.shutdown().await?;

    // Disconnect MCP
    if let Some(ref mut registry) = ctx.mcp_registry {
        let tools: Vec<_> = registry.list_tools();
        for tool_id in tools {
            registry.disconnect(&tool_id.server).await;
        }
    }

    Ok(())
}

async fn handle_repl(
    model: Option<String>,
    no_memory: bool,
    no_hitl: bool,
    hitl_mode: Option<String>,
    risk_threshold: Option<String>,
    no_router: bool,
    cli_path: Option<PathBuf>,
    continue_session: bool,
    resume: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Run first-run setup if needed
    let _ = first_run_setup(false);

    // Load settings
    let settings = match load_settings() {
        Ok(s) => s,
        Err(_) => {
            eprintln!("[ante] Using default settings");
            Settings::default()
        }
    };

    // Initialize context
    let mut ctx = AgentContext::initialize(settings);

    // Run MCP connection (best-effort)
    ctx.connect_mcp_servers().await;

    // Show startup banner
    ctx.show_banner();

    // Update status bar with MCP count, memory entries, and todos
    if let Some(ref reg) = ctx.mcp_registry {
        let tools: Vec<_> = reg.list_tools();
        let connected_count = tools
            .iter()
            .map(|t| &t.server)
            .collect::<std::collections::HashSet<_>>()
            .len();
        ctx.status_bar
            .set_mcp_servers(ctx.settings.mcp_servers.len(), connected_count);
    }
    if let Some(ref mem) = ctx.memory {
        ctx.status_bar.set_memory_entries(mem.search("").len());
    }
    if let Some(ref todos) = ctx.todo {
        let items = todos.list();
        let active = items.iter().filter(|i| !i.done).count() as u32;
        let done = items.iter().filter(|i| i.done).count() as u32;
        ctx.status_bar.set_todo_counts(active, done);
    }

    // Build Claude options
    let mut options = ClaudeOptions::default();
    options.cli_path = cli_path;
    if let Some(m) = model {
        options.model = Some(m);
    }
    if no_memory {
        ctx.memory = None;
    }
    if no_router {
        ctx.router = None;
    }

    // Apply HITL mode and risk threshold
    if let Some(ref mode_str) = hitl_mode {
        if let Some(mode) = HitlMode::from_str(mode_str) {
            if let Some(ref mut approval) = ctx.approval {
                *approval = std::mem::take(approval).with_mode(mode);
            }
        } else {
            eprintln!("[ante] Warning: unknown HITL mode '{mode_str}', using default");
        }
    }
    if let Some(ref threshold_str) = risk_threshold {
        if let Some(level) = RiskLevel::from_str(threshold_str) {
            if let Some(ref mut approval) = ctx.approval {
                *approval = std::mem::take(approval).with_risk_threshold(level);
            }
        } else {
            eprintln!("[ante] Warning: unknown risk threshold '{threshold_str}', using default");
        }
    }

    if no_hitl {
        ctx.approval = None;
    }

    // Run the REPL with session options
    run_repl_with_options(&mut ctx, options, continue_session, resume).await?;

    Ok(())
}

fn handle_memory_direct(command: MemoryCommands) -> Result<(), Box<dyn std::error::Error>> {
    // Use the shared memory path (settings default: ~/ai-wiki/.meta/ante-memory.db)
    use agent_sdk::settings::load_settings;
    let settings = load_settings().ok();
    let mem_path = settings
        .as_ref()
        .map(|s| expand_tilde_path(s.memory.db_path.clone()))
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
                .join("ai-wiki")
                .join(".meta")
                .join("ante-memory.db")
        });

    let max_memories = settings
        .as_ref()
        .map(|s| s.memory.max_context_memories)
        .unwrap_or(20);
    let mut store = MemoryStore::open(mem_path, max_memories)
        .map_err(|e| format!("Failed to open memory store: {e}"))?;

    match command {
        MemoryCommands::Add {
            content,
            tags,
            project,
        } => {
            let entry = store
                .add(content, tags, project)
                .map_err(|e| format!("Failed to add memory: {e}"))?;
            println!("Added memory: {} ({})", entry.id, entry.content);
        }
        MemoryCommands::Search { query } => {
            let results = store.search(&query);
            if results.is_empty() {
                println!("No memories found for: {query}");
            } else {
                for entry in results {
                    let ts = &entry.timestamp[..8.min(entry.timestamp.len())];
                    println!(
                        "[{ts}] [{}] {} (project: {})",
                        entry.tags, entry.content, entry.project
                    );
                }
            }
        }
        MemoryCommands::Context { project, max } => {
            let results = store.get_context(&project, max);
            if results.is_empty() {
                println!("No memories for project: {project}");
            } else {
                for entry in results {
                    let ts = &entry.timestamp[..8.min(entry.timestamp.len())];
                    println!("[{ts}] [{}] {}", entry.tags, entry.content);
                }
            }
        }
    }
    Ok(())
}

fn handle_sessions(command: SessionCommands) -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let sessions_root = PathBuf::from(&home).join(".ante").join("sessions");
    let mgr = SessionManager::new(sessions_root);

    match command {
        SessionCommands::List { project } => {
            let sessions = match project {
                Some(ref p) => mgr.list_sessions_for_project(Path::new(p))?,
                None => mgr.list_sessions()?,
            };

            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("Sessions:");
            println!();
            for s in &sessions {
                let dur = match (&s.started_at, &s.ended_at) {
                    (start, Some(end)) => {
                        // Crude duration estimate from timestamps
                        format!("{start} → {end}")
                    }
                    (start, None) => format!("{start} (active)"),
                };
                let model = s.model_id.as_deref().unwrap_or("?");
                let msgs = s.message_count;
                println!(
                    "  {:<38}  {:25}  {:12}  {} msgs  {}",
                    s.session_id, s.project, model, msgs, dur
                );
            }
            println!();
            println!("{} session(s) found.", sessions.len());
        }
        SessionCommands::Show { id, messages } => {
            match mgr.read_session(&id)? {
                None => {
                    eprintln!("Session not found: {id}");
                }
                Some(lines) => {
                    // Find the session header
                    let mut provider = "?".to_string();
                    let mut model = "?".to_string();
                    let mut cwd = "?".to_string();
                    let mut started = "?".to_string();

                    let msg_lines: Vec<String> = lines
                        .iter()
                        .filter_map(|line| match line {
                            agent_sdk::sessions::SessionLine::Session(h) => {
                                provider = h.provider.clone().unwrap_or_default();
                                model = h.model_id.clone().unwrap_or_default();
                                cwd = h.cwd.clone().unwrap_or_default();
                                started = h.timestamp.clone();
                                None
                            }
                            agent_sdk::sessions::SessionLine::Message(msg) => {
                                let role = &msg.message.role;
                                let content_str = match &msg.message.content {
                                    serde_json::Value::String(s) => s.clone(),
                                    serde_json::Value::Array(arr) => arr
                                        .iter()
                                        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                    other => other.to_string(),
                                };
                                let preview = content_str
                                    .lines()
                                    .next()
                                    .unwrap_or(&content_str)
                                    .chars()
                                    .take(120)
                                    .collect::<String>();
                                Some(format!("  [{role:12}] {preview}"))
                            }
                            _ => None,
                        })
                        .collect();

                    let total = msg_lines.len();
                    let shown: Vec<&String> = msg_lines.iter().rev().take(messages).rev().collect();

                    println!("Session: {}", id);
                    println!("  Project:  {cwd}");
                    println!("  Provider: {provider}");
                    println!("  Model:    {model}");
                    println!("  Start:    {started}");
                    println!("  Messages: {total} (showing last {})", shown.len());
                    println!();
                    for line in shown {
                        println!("{line}");
                    }
                }
            }
        }
        SessionCommands::Resume { id } => {
            eprintln!("[ante] To resume session {id}, run:");
            eprintln!("  ante --resume {id}");
            eprintln!();
            eprintln!("(Or use `ante --continue` to recover the latest session");
            eprintln!(" for the current directory.)");
        }
    }

    Ok(())
}

fn handle_todo_direct(command: TodoCommands) -> Result<(), Box<dyn std::error::Error>> {
    let ante_dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ante"))
        .unwrap_or_else(|_| PathBuf::from(".ante"));

    let todo_path = ante_dir.join("todo.json");
    let mut todos =
        TodoList::open(todo_path).map_err(|e| format!("Failed to open todo list: {e}"))?;

    match command {
        TodoCommands::Add { text } => {
            let text = text.join(" ");
            let item = todos
                .add(&text)
                .map_err(|e| format!("Failed to add todo: {e}"))?;
            println!("✅ Todo #{}: {}", item.id, item.text);
        }
        TodoCommands::List => {
            let items = todos.list();
            if items.is_empty() {
                println!("No todos.");
            } else {
                for item in items {
                    let status = if item.done { "✓" } else { " " };
                    println!("[{status}] #{} {}", item.id, item.text);
                }
            }
        }
        TodoCommands::Done { id } => {
            let item = todos.complete(id).map_err(|e| format!("Error: {e}"))?;
            println!("✅ Completed: {}", item.text);
        }
        TodoCommands::Clear => match todos.clear_done() {
            Ok(()) => println!("Cleared completed todos."),
            Err(e) => eprintln!("Error: {e}"),
        },
    }
    Ok(())
}

fn handle_doctor() -> Result<(), Box<dyn std::error::Error>> {
    let _ = first_run_setup(false);
    println!("Ante doctor");

    report_check(
        "root workspace",
        Path::new("Cargo.toml").exists(),
        "Cargo.toml present",
    );
    report_check(
        "claude cli",
        command_available("claude"),
        "claude found in PATH",
    );
    report_check(
        "opencode cli",
        command_available("opencode"),
        "opencode found in PATH",
    );

    let settings = load_settings().ok();
    report_check(
        "settings",
        settings.is_some(),
        "~/.ante/settings.json loadable",
    );

    let agents_dir = resolve_agents_dir(None)?;
    let agent_registry = AgentRegistry::load(&agents_dir).ok();
    report_check(
        "agents directory",
        agents_dir.exists(),
        &format!("{}", agents_dir.display()),
    );
    report_check(
        "agent registry",
        agent_registry
            .as_ref()
            .map(|registry| registry.count() > 0)
            .unwrap_or(false),
        &format!(
            "{} parseable agents",
            agent_registry
                .as_ref()
                .map(|registry| registry.count())
                .unwrap_or(0)
        ),
    );

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let ai_wiki = PathBuf::from(&home).join("ai-wiki");
    let wiki_repo = PathBuf::from(&home).join("code").join("wiki-memory");
    let wiki_dir = wiki_repo.join("wiki");
    let memory_path = settings
        .as_ref()
        .map(|s| expand_tilde_path(s.memory.db_path.clone()))
        .unwrap_or_else(|| {
            if wiki_repo.exists() {
                wiki_dir.join(".meta").join("ante-memory.db")
            } else {
                ai_wiki.join(".meta").join("ante-memory.db")
            }
        });
    report_check(
        "wiki-memory repo",
        wiki_repo.exists(),
        &format!("{}", wiki_repo.display()),
    );
    report_check(
        "wiki-memory wiki dir",
        wiki_dir.exists(),
        &format!("{}", wiki_dir.display()),
    );

    let uses_direct_wiki = memory_path.starts_with(&wiki_dir);
    let ai_wiki_ok = uses_direct_wiki
        || if ai_wiki.exists() {
            match fs::read_link(&ai_wiki) {
                Ok(target) => target == wiki_dir,
                Err(_) => false,
            }
        } else {
            false
        };
    let wiki_link_detail = if uses_direct_wiki {
        "using wiki-memory directly".to_string()
    } else {
        format!("{} -> {}", ai_wiki.display(), wiki_dir.display())
    };
    report_check("wiki-memory link", ai_wiki_ok, &wiki_link_detail);
    report_check(
        "memory path",
        memory_path.parent().map(|p| p.exists()).unwrap_or(false),
        &format!("{}", memory_path.display()),
    );
    let memory_open = MemoryStore::open(memory_path.clone(), 1).is_ok();
    report_check("memory store", memory_open, "open shared memory database");

    let sessions_dir = PathBuf::from(&home).join(".ante").join("sessions");
    report_check(
        "sessions directory",
        sessions_dir.exists() || fs::create_dir_all(&sessions_dir).is_ok(),
        &format!("{}", sessions_dir.display()),
    );

    let hook_dir = PathBuf::from(&home).join(".ante").join("hooks");
    for hook in ["block-danger.sh", "pre_compact.py", "session_end.py"] {
        let hook_path = hook_dir.join(hook);
        report_check(
            &format!("hook {hook}"),
            hook_path.exists() && is_executable(&hook_path),
            &format!("{}", hook_path.display()),
        );
    }

    report_check(
        "internal MCP tools",
        probe_internal_mcp_tools(),
        "tools/list exposes built-in tools",
    );

    if !ai_wiki_ok && ai_wiki.exists() {
        println!(
            "WARN ai-wiki exists but is not linked to wiki-memory. Migrate manually after backing up: {}",
            ai_wiki.display()
        );
    }

    Ok(())
}

fn report_check(name: &str, ok: bool, detail: &str) {
    let status = if ok { "OK" } else { "WARN" };
    println!("{status:4} {name}: {detail}");
}

fn command_available(command: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command} >/dev/null 2>&1"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

fn probe_internal_mcp_tools() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Ok(mut child) = Command::new(exe)
        .arg("internal-mcp-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    if let Some(stdin) = child.stdin.as_mut() {
        let _ = writeln!(
            stdin,
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{}}}}"#
        );
    }
    drop(child.stdin.take());

    let Ok(output) = child.wait_with_output() else {
        return false;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    output.status.success()
        && stdout.contains("memory_add")
        && stdout.contains("memory_search")
        && stdout.contains("memory_get_context")
}

fn handle_agents(command: AgentsCommands) -> Result<(), Box<dyn std::error::Error>> {
    let _ = first_run_setup(false);

    match command {
        AgentsCommands::List { agent_dir } => {
            let agents_dir = resolve_agents_dir(agent_dir)?;
            let registry = load_agent_registry(&agents_dir)?;

            let agents = registry.all();
            if agents.is_empty() {
                println!("No agent files found in {}", agents_dir.display());
                println!("Place .md files with YAML frontmatter in the agents directory.");
            } else {
                for agent in agents {
                    println!("  {} — {}", agent.name, agent.description);
                }
            }
        }
        AgentsCommands::Match { agent_dir, task } => {
            let task = task.join(" ");
            if task.trim().is_empty() {
                return Err("agents match requires a task".into());
            }
            let agents_dir = resolve_agents_dir(agent_dir)?;
            let registry = load_agent_registry(&agents_dir)?;
            print_agent_match(&registry, &task);
        }
        AgentsCommands::Run {
            backend,
            model,
            agent_dir,
            cwd,
            output,
            dry_run,
            read_only,
            skip_permissions,
            task,
        } => {
            let task = task.join(" ");
            if task.trim().is_empty() {
                return Err("agents run requires a task".into());
            }

            let agents_dir = resolve_agents_dir(agent_dir)?;
            eprintln!("[ante] Loading agents from: {}", agents_dir.display());
            let registry = load_agent_registry(&agents_dir)?;
            let Some(agent) = registry.find_best_match(&task) else {
                println!("No matching agent found for: {task}");
                println!("Available agents:");
                for agent in registry.all() {
                    println!("  {} — {}", agent.name, agent.description);
                }
                return Ok(());
            };

            let run_dir = cwd
                .map(expand_tilde_path)
                .unwrap_or(std::env::current_dir()?);
            let selected_model = model
                .or_else(|| agent.model.clone())
                .or_else(|| std::env::var("ANTE_AGENT_MODEL").ok())
                .unwrap_or_else(|| "opencode/deepseek-v4-flash".to_string());
            let rendered_prompt = render_agent_task_prompt(agent, &task, read_only);

            if dry_run || backend == "dry-run" {
                println!("Best match: {} — {}", agent.name, agent.description);
                println!("Backend: {backend}");
                println!("Model: {}", normalize_opencode_model(&selected_model));
                println!("CWD: {}", run_dir.display());
                println!("\nRendered prompt:\n{rendered_prompt}");
                return Ok(());
            }

            match backend.as_str() {
                "opencode" => run_opencode_agent(
                    agent,
                    &rendered_prompt,
                    &selected_model,
                    &run_dir,
                    output,
                    skip_permissions,
                )?,
                other => {
                    return Err(format!(
                        "unsupported agent backend '{other}'. Supported backends: opencode, dry-run"
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

fn resolve_agents_dir(
    override_dir: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(dir) = override_dir {
        return Ok(expand_tilde_path(dir));
    }

    let settings = load_settings().ok();
    if let Some(settings) = settings {
        return Ok(expand_tilde_path(settings.agents.directory));
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    Ok(PathBuf::from(home).join(".ante").join("agents"))
}

fn load_agent_registry(agents_dir: &Path) -> Result<AgentRegistry, Box<dyn std::error::Error>> {
    if !agents_dir.exists() {
        println!("No agents directory at: {}", agents_dir.display());
        println!("Run `ante init` to create it.");
        return Ok(AgentRegistry::load(agents_dir)?);
    }

    AgentRegistry::load(agents_dir)
        .map_err(|e| format!("Failed to load agents from {}: {e}", agents_dir.display()).into())
}

fn print_agent_match(registry: &AgentRegistry, task: &str) {
    match registry.find_best_match(task) {
        Some(agent) => {
            println!("Best match: {} — {}", agent.name, agent.description);
            if !agent.prompt.is_empty() {
                println!("\nSystem prompt:\n{}", agent.prompt);
            }
        }
        None => {
            println!("No matching agent found for: {task}");
            println!("Available agents:");
            for agent in registry.all() {
                println!("  {} — {}", agent.name, agent.description);
            }
        }
    }
}

fn render_agent_task_prompt(
    agent: &agent_sdk::agents::loader::SubAgent,
    task: &str,
    read_only: bool,
) -> String {
    let mut prompt = String::new();
    if !agent.prompt.trim().is_empty() {
        prompt.push_str(agent.prompt.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str("You are running as an Ante sub-agent.\n");
    prompt.push_str(&format!("Agent: {}\n", agent.name));
    prompt.push_str(&format!("Task: {task}\n"));
    if !agent.tools.is_empty() {
        prompt.push_str(&format!("Requested tools: {}\n", agent.tools.join(", ")));
    }
    if read_only {
        prompt.push_str(
            "Constraint: read-only inspection only. Do not create, edit, move, or delete files.\n",
        );
    }
    prompt.push_str(
        "Return a concise result with findings, files touched, and any remaining risks.\n",
    );
    prompt
}

fn run_opencode_agent(
    agent: &agent_sdk::agents::loader::SubAgent,
    prompt: &str,
    model: &str,
    cwd: &Path,
    output: Option<PathBuf>,
    skip_permissions: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !cwd.exists() {
        return Err(format!("agent cwd does not exist: {}", cwd.display()).into());
    }

    let model = normalize_opencode_model(model);
    let mut cmd = Command::new("opencode");
    cmd.arg("run")
        .arg("--model")
        .arg(&model)
        .arg("--dir")
        .arg(cwd)
        .arg("--title")
        .arg(format!("ante:{}", agent.name))
        .arg("--format")
        .arg("default");
    if skip_permissions {
        cmd.arg("--dangerously-skip-permissions");
    }
    cmd.arg(prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    eprintln!(
        "[ante] Running agent '{}' with OpenCode model '{}' in {}",
        agent.name,
        model,
        cwd.display()
    );
    let started = Instant::now();
    let result = cmd
        .output()
        .map_err(|e| format!("failed to launch opencode. Is it installed and in PATH? {e}"))?;
    let elapsed_ms = started.elapsed().as_millis();

    let stdout = String::from_utf8_lossy(&result.stdout);
    let stderr = String::from_utf8_lossy(&result.stderr);
    let error_detected = opencode_output_has_error(&stderr);
    let transcript = format!(
        "# Ante Agent Run\n\nagent: {}\nbackend: opencode\nmodel: {}\ncwd: {}\nstatus: {}\n\n## Stdout\n\n{}\n\n## Stderr\n\n{}\n",
        agent.name,
        model,
        cwd.display(),
        result.status,
        stdout,
        stderr,
    );
    let summary = serde_json::json!({
        "agent": agent.name,
        "backend": "opencode",
        "model": model,
        "cwd": cwd.display().to_string(),
        "status": result.status.to_string(),
        "success": result.status.success() && !error_detected,
        "errorDetected": error_detected,
        "elapsedMs": elapsed_ms,
        "stdoutBytes": result.stdout.len(),
        "stderrBytes": result.stderr.len(),
    });

    if let Some(path) = output {
        let path = expand_tilde_path(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, transcript)?;
        let json_path = path.with_extension("json");
        fs::write(&json_path, serde_json::to_string_pretty(&summary)?)?;
        println!("Wrote agent transcript: {}", path.display());
        println!("Wrote agent summary: {}", json_path.display());
    } else {
        print!("{stdout}");
        if !stderr.trim().is_empty() {
            eprint!("{stderr}");
        }
    }

    if !result.status.success() || error_detected {
        return Err(format!(
            "opencode agent '{}' failed with {}",
            agent.name, result.status
        )
        .into());
    }

    Ok(())
}

fn normalize_opencode_model(model: &str) -> String {
    if model.contains('/') {
        model.to_string()
    } else if model == "deepseek-v4-flash" {
        "opencode/deepseek-v4-flash".to_string()
    } else {
        model.to_string()
    }
}

fn opencode_output_has_error(stderr: &str) -> bool {
    stderr.contains("ProviderModelNotFoundError")
        || stderr.contains("Model not found")
        || stderr.contains("Insufficient balance")
        || stderr.contains("Authentication")
        || stderr.contains("authentication_failed")
        || stderr.contains("API key")
}

fn expand_tilde_path(path: PathBuf) -> PathBuf {
    let Some(raw) = path.to_str() else {
        return path;
    };
    if raw == "~" {
        return std::env::var("HOME").map(PathBuf::from).unwrap_or(path);
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path
}

fn handle_diagram(source: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let source = source.join(" ");
    if source.is_empty() {
        eprintln!("Usage: ante diagram <mermaid source>");
        return Ok(());
    }
    match render(&source) {
        Ok(ascii) => println!("{ascii}"),
        Err(e) => eprintln!("Diagram error: {e}"),
    }
    Ok(())
}
