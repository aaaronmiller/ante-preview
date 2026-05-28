//! Ante — extensible agent runtime wrapping Claude Code.
//!
//! Modes:
//!   ante query <prompt>  — One-shot query with full Ante tooling
//!   ante repl            — Interactive session with extensibility features
//!   ante init            — First-run setup
//!   ante memory <cmd>    — Direct memory operations
//!   ante todo <cmd>      — Direct todo list operations
//!   ante agents <cmd>    — Sub-agent management
//!   ante diagram <file>  — Render Mermaid file to ASCII

mod status;
mod mcp_server;

use std::io::{self, Write};
use std::path::PathBuf;

use status::{render_banner, StatusBar};

use agent_sdk::budget::{BudgetConfig, BudgetTracker};
use agent_sdk::claude::{
    Claude, ClaudeMessage, ClaudeOptions, ContentBlock, ControlRequestMessage,
    ControlResponseMessage, ResultMessage, UserMessage, AssistantMessage,
    SystemMessage, StreamEventMessage,
};
use agent_sdk::event::EventBus;
use agent_sdk::hitl::{ApprovalManager, ApprovalDecision, HitlMode, RiskLevel};
use agent_sdk::hooks::registry::HookRegistry;
use agent_sdk::init::first_run_setup;
use agent_sdk::mcp::registry::{McpServerConfigEntry, McpToolRegistry};
use agent_sdk::memory::store::MemoryStore;
use agent_sdk::memory::server::MemoryServer;
use agent_sdk::router::ModelRouter;
use agent_sdk::settings::load_settings;
use agent_sdk::ui::diagram::render;
use agent_sdk::ui::todo::TodoList;
use agent_sdk::agents::loader::AgentRegistry;
use ante_protocol_shape::settings::Settings;
use ante_protocol_shape::{
    BasePayload, EventPayload, Id,
    SessionStartPayload, SessionEndPayload,
    CompactPayload, UserPromptPayload, PermissionRequestPayload,
};
use ante_protocol_shape::payload::RiskLevel as ProtocolRiskLevel;
use clap::{Parser, Subcommand};

// ─── CLI ────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "ante", about = "Ante — extensible agent runtime")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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

    /// Interactive REPL session
    Repl {
        /// Model override
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

        /// Disable model routing
        #[arg(long)]
        no_router: bool,

        /// Path to Claude CLI binary
        #[arg(long)]
        cli_path: Option<PathBuf>,
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
    List,
    /// Decompose and run a task through sub-agents
    Run { task: Vec<String> },
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
}

impl AgentContext {
    /// Initialize all components from settings.
    fn initialize(settings: Settings) -> Self {
        let ante_dir = settings
            .ante_dir
            .clone()
            .unwrap_or_else(|| {
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
        let memory = MemoryStore::open(
            settings.memory.db_path.clone(),
            settings.memory.max_context_memories,
        )
        .ok();

        let memory_server = memory.as_ref().map(|_| {
            MemoryServer::open(
                settings.memory.db_path.clone(),
                settings.memory.max_context_memories,
            )
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
        }
    }

    /// Connect MCP servers from settings and register internal tools.
    async fn connect_mcp_servers(&mut self) {
        let mut registry = McpToolRegistry::new();

        // ── Internal Ante tools server (diagram + todo) ──────────────────
        let internal_entry = McpServerConfigEntry {
            name: "ante-tools".to_string(),
            command: std::env::current_exe().ok()
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
        let memory_count = self.memory.as_ref()
            .map(|m| m.search("").len())
            .unwrap_or(0);
        let agent_count = match agent_sdk::agents::loader::AgentRegistry::load(
            &self.ante_dir.join("agents")
        ) {
            Ok(reg) => reg.count(),
            Err(_) => 0,
        };
        eprint!("{}", render_banner(
            env!("CARGO_PKG_VERSION"),
            model,
            self.settings.mcp_servers.len(),
            agent_count,
            memory_count,
        ));
    }

    /// Render and print the current status bar to stderr.
    fn print_status(&self) {
        let line = self.status_bar.render();
        eprint!("\r{}", line);
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
            let tags = if content.contains("config") || content.contains("port") || content.contains("env") {
                "config"
            } else if content.contains("api") || content.contains("token") || content.contains("key") {
                "api"
            } else if content.contains("bug") || content.contains("fix") || content.contains("issue") {
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
    async fn check_hitl(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
    ) -> Result<(), String> {
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

        eprintln!(
            "\n⚠️  Tool requires approval: {tool_name} ({risk:?} risk)"
        );
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

async fn run_repl(ctx: &mut AgentContext, cli_options: ClaudeOptions) -> Result<(), Box<dyn std::error::Error>> {
    // Connect MCP servers
    ctx.connect_mcp_servers().await;

    let bp = base_payload(&std::env::current_dir().unwrap_or_default());

    // Fire session start event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus.emit(&EventPayload::SessionStart(session_start_payload(&bp))).await;
    }

    eprintln!("Connecting to Claude CLI...");
    let mut client = Claude::connect(cli_options).await?;
    let model_name = client.server_info()
        .and_then(|info| info.get("model"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "claude".to_string());
    ctx.status_bar.set_model(&model_name);
    ctx.print_status();
    eprintln!("Connected. Type /help for commands.\n");

    let mut line = String::new();
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
            let should_quit = handle_command(
                input, &mut client, ctx, &bp,
            ).await?;
            if should_quit {
                break;
            }
            continue;
        }

        // Fire PreUserPromptSubmit event
        if let Some(ref bus) = ctx.event_bus {
            let payload = EventPayload::PreUserPromptSubmit(
                user_prompt_payload(&bp, input)
            );
            let result = bus.emit(&payload).await;
            if !result.decision.is_allowed() {
                eprintln!("[ante] Prompt blocked by hook: {:?}", result.hooks_executed);
                continue;
            }
        }

        // Check for memory context injection at first prompt
        let has_memory_context = {
            let project = PathBuf::from(".")
                .canonicalize()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "default".to_string());

            if let Some(ctx_text) = ctx.get_memory_context(&project) {
                // Send memory context as a system preamble before the user's prompt
                let combined = format!("{}\n\n{}", ctx_text, input);
                client.send_user_text(combined).await?;
                true
            } else {
                false
            }
        };

        if !has_memory_context {
            client.send_user_text(input).await?;
        }

        println!();
        stream_response(&mut client, ctx, &bp).await?;
        println!();

        // Fire PostUserPromptSubmit event
        if let Some(ref bus) = ctx.event_bus {
            let payload = EventPayload::PostUserPromptSubmit(
                user_prompt_payload(&bp, input)
            );
            let _ = bus.emit(&payload).await;
        }
    }

    // Fire session end event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus.emit(&EventPayload::SessionEnd(session_end_payload(&bp))).await;
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
                    if let Some(output_tokens) = usage.get("output_tokens").and_then(|v| v.as_u64()) {
                        ctx.budget_snapshot.output_tokens += output_tokens;
                    }
                }
                if let Some(cost) = result.total_cost_usd {
                    ctx.budget.add_cost(cost);
                    ctx.budget_snapshot.total_cost += cost;
                }

                // Update status bar with latest usage
                ctx.status_bar.add_tokens(
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
                    let payload = EventPayload::PermissionRequest(
                        PermissionRequestPayload {
                            base: bp.clone(),
                            tool_name: name.clone(),
                            input: tool_input.clone().unwrap_or(serde_json::Value::Null),
                            risk_level: ProtocolRiskLevel::Medium,
                            message: format!("Tool '{}' requires permission", name),
                            can_modify: true,
                        }
                    );
                    let result = bus.emit(&payload).await;
                    if !result.decision.is_allowed() {
                        client
                            .respond_control_request_error(request_id, &format!("Blocked by hook: {:?}", result.hooks_executed))
                            .await?;
                        return Ok(());
                    }
                }
            }

            // Check HITL approval
            if let (Some(name), Some(input)) = (&tool_name, &tool_input) {
                if let Err(reason) = ctx.check_hitl(name, input).await {
                    client
                        .respond_control_request_error(request_id, &format!("Denied by HITL: {reason}"))
                        .await?;
                    return Ok(());
                }
            }

            // Default: allow if HITL passes
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
                .respond_control_request_error(request_id, &format!("unsupported control request type: {other}"))
                .await?;
        }
    }

    Ok(())
}

/// Extract tool name and input from a control request payload.
fn extract_tool_request(request: &Option<serde_json::Value>) -> (Option<String>, Option<serde_json::Value>) {
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
                        eprintln!("  [{ts}] [{}] {} (project: {})", entry.tags, entry.content, entry.project);
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
                let pretty = serde_json::to_string_pretty(value)
                    .unwrap_or_else(|_| value.to_string());
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
        let pretty = serde_json::to_string_pretty(&message.raw)
            .unwrap_or_else(|_| message.raw.to_string());
        for line in pretty.lines() {
            println!("  {line}");
        }
    }
}

fn render_system(message: &SystemMessage) {
    print_header("system", message.subtype.as_deref());
    let pretty = serde_json::to_string_pretty(&message.raw)
        .unwrap_or_else(|_| message.raw.to_string());
    for line in pretty.lines() {
        println!("  {line}");
    }
}

fn render_stream_event(message: &StreamEventMessage) {
    print_header("stream_event", None);
    if let Some(event) = &message.event {
        let pretty = serde_json::to_string_pretty(event)
            .unwrap_or_else(|_| event.to_string());
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
        let pretty = serde_json::to_string_pretty(response)
            .unwrap_or_else(|_| response.to_string());
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
        let pretty = serde_json::to_string_pretty(usage)
            .unwrap_or_else(|_| usage.to_string());
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
                let pretty = serde_json::to_string_pretty(other)
                    .unwrap_or_else(|_| other.to_string());
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

    match cli.command {
        Commands::Init { force } => {
            handle_init(force)?
        }
        Commands::Query {
            prompt,
            model,
            no_memory,
            no_hitl,
            hitl_mode,
            risk_threshold,
            no_router,
        } => {
            handle_query(prompt, model, no_memory, no_hitl, hitl_mode, risk_threshold, no_router).await?;
        }
        Commands::Repl {
            model,
            no_memory,
            no_hitl,
            hitl_mode,
            risk_threshold,
            no_router,
            cli_path,
        } => {
            handle_repl(model, no_memory, no_hitl, hitl_mode, risk_threshold, no_router, cli_path).await?;
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

    // Update status bar with MCP count
    if let Some(ref reg) = ctx.mcp_registry {
        let tools: Vec<_> = reg.list_tools();
        ctx.status_bar.set_mcp_servers(
            tools.iter().map(|t| &t.server).collect::<std::collections::HashSet<_>>().len()
        );
    }

    // Build Claude options
    let mut options = ClaudeOptions::default();

    // Model router
    let selected_model = if !no_router {
        if let Some(ref router) = ctx.router {
            match router.select(&prompt_text, 0) {
                Ok(decision) => {
                    eprintln!("[ante] Model router: {} ({})", decision.selected_model, decision.reason);
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
        let _ = bus.emit(&EventPayload::SessionStart(session_start_payload(&bp))).await;
    }

    // Fire PreUserPromptSubmit event
    let prompt_allowed = if let Some(ref bus) = ctx.event_bus {
        let payload = EventPayload::PreUserPromptSubmit(
            user_prompt_payload(&bp, &prompt_text)
        );
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

    // Send query
    client.send_user_text(&prompt_text).await?;

    // Fire PostUserPromptSubmit event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus.emit(&EventPayload::PostUserPromptSubmit(
            user_prompt_payload(&bp, &prompt_text)
        )).await;
    }

    // Stream response, handling control requests etc.
    stream_response(&mut client, &mut ctx, &bp).await?;

    // Fire SessionEnd event
    if let Some(ref bus) = ctx.event_bus {
        let _ = bus.emit(&EventPayload::SessionEnd(session_end_payload(&bp))).await;
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

    // Update status bar with MCP count
    if let Some(ref reg) = ctx.mcp_registry {
        let tools: Vec<_> = reg.list_tools();
        ctx.status_bar.set_mcp_servers(
            tools.iter().map(|t| &t.server).collect::<std::collections::HashSet<_>>().len()
        );
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

    // Run the REPL
    run_repl(&mut ctx, options).await?;

    Ok(())
}

fn handle_memory_direct(command: MemoryCommands) -> Result<(), Box<dyn std::error::Error>> {
    let ante_dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ante"))
        .unwrap_or_else(|_| PathBuf::from(".ante"));

    let mem_path = ante_dir.join("memory").join("ante-memory.db");
    let mut store = MemoryStore::open(mem_path, 20)
        .map_err(|e| format!("Failed to open memory store: {e}"))?;

    match command {
        MemoryCommands::Add { content, tags, project } => {
            let entry = store.add(content, tags, project)
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
                    println!("[{ts}] [{}] {} (project: {})", entry.tags, entry.content, entry.project);
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

fn handle_todo_direct(command: TodoCommands) -> Result<(), Box<dyn std::error::Error>> {
    let ante_dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ante"))
        .unwrap_or_else(|_| PathBuf::from(".ante"));

    let todo_path = ante_dir.join("todo.json");
    let mut todos = TodoList::open(todo_path)
        .map_err(|e| format!("Failed to open todo list: {e}"))?;

    match command {
        TodoCommands::Add { text } => {
            let text = text.join(" ");
            let item = todos.add(&text)
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
            let item = todos.complete(id)
                .map_err(|e| format!("Error: {e}"))?;
            println!("✅ Completed: {}", item.text);
        }
        TodoCommands::Clear => {
            match todos.clear_done() {
                Ok(()) => println!("Cleared completed todos."),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
    }
    Ok(())
}

fn handle_agents(command: AgentsCommands) -> Result<(), Box<dyn std::error::Error>> {
    let ante_dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ante"))
        .unwrap_or_else(|_| PathBuf::from(".ante"));

    match command {
        AgentsCommands::List => {
            let agents_dir = ante_dir.join("agents");
            if !agents_dir.exists() {
                println!("No agents directory at: {}", agents_dir.display());
                println!("Run `ante init` to create it.");
                return Ok(());
            }

            let registry = AgentRegistry::load(&agents_dir)
                .map_err(|e| format!("Failed to load agents: {e}"))?;

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
        AgentsCommands::Run { task } => {
            let task = task.join(" ");
            let agents_dir = ante_dir.join("agents");
            eprintln!("[ante] Loading agents from: {}", agents_dir.display());

            let registry = AgentRegistry::load(&agents_dir)
                .map_err(|e| format!("Failed to load agents: {e}"))?;

            let agent = registry.find_best_match(&task);
            match agent {
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
    }
    Ok(())
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
