/// Startup ASCII banner + session status bar for Ante.
///
/// The status bar uses a balanced 6-color semantic palette:
///
///   Cyan   #00BCD4   Primary — model name, labels
///   Green  #4CAF50   OK — normal context, zero failures
///   Yellow #FFC107   Warning — context >50%, budget near limit
///   Red    #F44336   Critical — context >80%, failures, over budget
///   Blue   #2196F3   Stats — tokens, turns, rates
///   Purple #9C27B0   Agents — sub-agent counts
///   Gray   #9E9E9E   Muted — secondary info, separators
///
/// No rainbow vomit. Every color has a job. Layout adapts to terminal width:
/// single-line ≥32 chars, two-line when color is on and terminal is ≥80 wide.
use std::time::Instant;

// ─── ANSI Color Constants ───────────────────────────────────────────────────

const C_CYAN: &str = "\x1b[38;2;0;188;212m";
const C_GREEN: &str = "\x1b[38;2;76;175;80m";
const C_YELLOW: &str = "\x1b[38;2;255;193;7m";
const C_RED: &str = "\x1b[38;2;244;67;54m";
const C_BLUE: &str = "\x1b[38;2;33;150;243m";
const C_PURPLE: &str = "\x1b[38;2;156;39;176m";
const C_GRAY: &str = "\x1b[38;2;158;158;158m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Wrap text in an ANSI color tag. No-op when color is disabled.
fn color(c: &str, text: &str, use_color: bool) -> String {
    if use_color {
        format!("{c}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn dim(text: &str, use_color: bool) -> String {
    if use_color {
        format!("{DIM}{text}{RESET}")
    } else {
        text.to_string()
    }
}

// ─── Segment Separator ──────────────────────────────────────────────────────

/// Thin vertical separator between status bar segments.
const SEP: &str = "▎";

// ─── Banner ─────────────────────────────────────────────────────────────────

/// Render the startup banner to a string.
pub fn render_banner(
    version: &str,
    model: Option<&str>,
    mcp_count: usize,
    agent_count: usize,
    memory_count: usize,
) -> String {
    let mut s = String::new();

    s.push_str(
        r#"
    █████╗ ███╗   ██╗████████╗███████╗
   ██╔══██╗████╗  ██║╚══██╔══╝██╔════╝
   ███████║██╔██╗ ██║   ██║   █████╗
   ██╔══██║██║╚██╗██║   ██║   ██╔══╝
   ██║  ██║██║ ╚████║   ██║   ███████╗
   ╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   ╚══════╝
"#,
    );

    s.push_str("   ═══════════════════════════════════════════════════\n");
    s.push_str(&format!("   v{:<47}\n", version));

    if let Some(model) = model {
        s.push_str(&format!("   model │ {:<44}\n", model));
    }

    s.push_str("   ─────────────────────────────────────────────────\n");

    let badge = |name: &str, count: usize, enabled: bool| -> String {
        if enabled {
            format!("[✓] {name} {count} ")
        } else {
            format!("[ ] {name} - ")
        }
    };

    s.push_str("   ");
    s.push_str(&badge("hooks", 1, true));
    s.push_str(&badge("mcp", mcp_count, mcp_count > 0));
    s.push_str("\n   ");
    s.push_str(&badge("agents", agent_count, agent_count > 0));
    s.push_str(&badge("memories", memory_count, memory_count > 0));
    s.push_str("\n");

    s.push_str("   ═══════════════════════════════════════════════════\n");
    s
}

// ─── StatusBar ──────────────────────────────────────────────────────────────

/// Session status bar with expanded tracking, semantic colors, and adaptive layout.
///
/// Tracks everything you need to know at a glance:
/// - Model, context %, cost, elapsed time (core session info)
/// - Tool call success/failure/blocked counts
/// - Sub-agent work statistics
/// - MCP server connectivity
/// - Hook pipeline decisions
/// - HITL approval activity
/// - Memory and todo counts
/// - Token/cost budget remaining
///
/// # Adaptive layout
/// - `render()`         — Compact single line (≥32 chars), always works.
/// - `render_detailed()` — Two-line detailed view (≥80 chars recommended).
///
/// # Balanced color palette
/// Every color has a semantic meaning. No gratuitous rainbow.
pub struct StatusBar {
    // Configuration
    color: bool,
    start: Instant,

    // ── Core session ────────────────────────────────────────────────────
    model: String,
    token_prompt: u64,
    token_completion: u64,
    max_context: u64,
    cost: f64,
    max_cost: f64,

    // ── MCP ─────────────────────────────────────────────────────────────
    mcp_servers_configured: usize,
    mcp_servers_connected: usize,

    // ── Memory ──────────────────────────────────────────────────────────
    memory_entries: usize,

    // ── Sub-agents ──────────────────────────────────────────────────────
    subagent_active: u32,
    subagent_completed: u32,
    subagent_failed: u32,

    // ── Tool calls ──────────────────────────────────────────────────────
    tool_calls_ok: u32,
    tool_calls_failed: u32,
    tool_calls_blocked: u32,

    // ── Hooks ───────────────────────────────────────────────────────────
    hooks_fired: u32,
    hooks_blocked: u32,

    // ── HITL ────────────────────────────────────────────────────────────
    risk_level: String,
    hitl_approved: u32,
    hitl_denied: u32,

    // ── Session ─────────────────────────────────────────────────────────
    turn_count: u32,

    // ── Todo ────────────────────────────────────────────────────────────
    todo_active: u32,
    todo_done: u32,
}

#[allow(dead_code)]
impl StatusBar {
    /// Create a new status bar. Color defaults to true (auto-detected).
    pub fn new(model: Option<&str>) -> Self {
        StatusBar {
            color: true,
            start: Instant::now(),
            model: model.unwrap_or("—").to_string(),
            token_prompt: 0,
            token_completion: 0,
            max_context: 200_000,
            cost: 0.0,
            max_cost: f64::MAX,
            mcp_servers_configured: 0,
            mcp_servers_connected: 0,
            memory_entries: 0,
            subagent_active: 0,
            subagent_completed: 0,
            subagent_failed: 0,
            tool_calls_ok: 0,
            tool_calls_failed: 0,
            tool_calls_blocked: 0,
            hooks_fired: 0,
            hooks_blocked: 0,
            risk_level: "safe".into(),
            hitl_approved: 0,
            hitl_denied: 0,
            turn_count: 0,
            todo_active: 0,
            todo_done: 0,
        }
    }

    /// Enable or disable ANSI color output.
    pub fn set_color(&mut self, enabled: bool) {
        self.color = enabled;
    }

    /// Whether color output is enabled.
    pub fn is_color(&self) -> bool {
        self.color
    }

    // ─── Setters ────────────────────────────────────────────────────────────

    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    pub fn set_max_context(&mut self, max: u64) {
        self.max_context = max;
    }

    pub fn set_max_cost(&mut self, max: f64) {
        self.max_cost = max;
    }

    /// Set cumulative usage values (replaces, not additive).
    /// Call this each turn with the running totals.
    pub fn set_usage(&mut self, prompt: u64, completion: u64, cost: f64) {
        self.token_prompt = prompt;
        self.token_completion = completion;
        self.cost = cost;
    }

    /// Add per-turn deltas. Use `set_usage` for cumulative totals.
    pub fn add_tokens(&mut self, prompt: u64, completion: u64, cost: f64) {
        self.token_prompt += prompt;
        self.token_completion += completion;
        self.cost += cost;
    }

    pub fn add_turn(&mut self) {
        self.turn_count += 1;
    }

    pub fn track_tool_ok(&mut self) {
        self.tool_calls_ok += 1;
    }

    pub fn track_tool_failure(&mut self) {
        self.tool_calls_failed += 1;
    }

    pub fn track_tool_blocked(&mut self) {
        self.tool_calls_blocked += 1;
    }

    pub fn track_hook_fired(&mut self) {
        self.hooks_fired += 1;
    }

    pub fn track_hook_blocked(&mut self) {
        self.hooks_fired += 1;
        self.hooks_blocked += 1;
    }

    pub fn track_hitl_approved(&mut self) {
        self.hitl_approved += 1;
    }

    pub fn track_hitl_denied(&mut self) {
        self.hitl_denied += 1;
    }

    pub fn set_mcp_servers(&mut self, configured: usize, connected: usize) {
        self.mcp_servers_configured = configured;
        self.mcp_servers_connected = connected;
    }

    pub fn set_memory_entries(&mut self, n: usize) {
        self.memory_entries = n;
    }

    pub fn set_todo_counts(&mut self, active: u32, done: u32) {
        self.todo_active = active;
        self.todo_done = done;
    }

    pub fn set_subagent_stats(&mut self, active: u32, completed: u32, failed: u32) {
        self.subagent_active = active;
        self.subagent_completed = completed;
        self.subagent_failed = failed;
    }

    pub fn set_risk_level(&mut self, level: &str) {
        self.risk_level = level.to_string();
    }

    // ─── Computed ───────────────────────────────────────────────────────────

    fn context_pct(&self) -> u32 {
        let total = self.token_prompt + self.token_completion;
        if self.max_context > 0 {
            ((total as f64 / self.max_context as f64) * 100.0).min(100.0) as u32
        } else {
            0
        }
    }

    fn budget_pct(&self) -> f64 {
        if self.max_cost > 0.0 && self.max_cost < f64::MAX {
            (self.cost / self.max_cost * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    fn turns_per_sec(&self) -> f64 {
        let secs = self.start.elapsed().as_secs_f64();
        if secs > 0.0 {
            self.turn_count as f64 / secs
        } else {
            0.0
        }
    }

    fn total_tool_calls(&self) -> u32 {
        self.tool_calls_ok + self.tool_calls_failed + self.tool_calls_blocked
    }

    fn total_subagents(&self) -> u32 {
        self.subagent_active + self.subagent_completed + self.subagent_failed
    }

    fn context_color(&self) -> &'static str {
        let pct = self.context_pct();
        if pct > 80 {
            C_RED
        } else if pct > 50 {
            C_YELLOW
        } else {
            C_GREEN
        }
    }

    fn failure_color(&self) -> &'static str {
        if self.tool_calls_failed > 0 {
            C_RED
        } else {
            C_GREEN
        }
    }

    fn budget_color(&self) -> &'static str {
        let pct = self.budget_pct();
        if pct > 80.0 {
            C_RED
        } else if pct > 50.0 {
            C_YELLOW
        } else {
            C_GREEN
        }
    }

    // ─── Context Bar ────────────────────────────────────────────────────────

    fn context_bar(&self) -> String {
        let pct = self.context_pct();
        let bar_width = 8usize;
        let filled = ((pct as f64 / 100.0) * bar_width as f64).round() as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width - filled;
        let filled_char = color(C_CYAN, "━", self.color);
        let empty_char = dim("━", self.color);
        let mut bar = String::with_capacity(bar_width * 3);
        for _ in 0..filled {
            bar.push_str(&filled_char);
        }
        for _ in 0..empty {
            bar.push_str(&empty_char);
        }
        bar
    }

    // ─── Session Indicator ──────────────────────────────────────────────────

    fn indicator(&self) -> String {
        let pct = self.context_pct();
        if self.tool_calls_failed > 0 || self.tool_calls_blocked > 0 {
            // Red diamond for failures
            color(C_RED, "◆", self.color)
        } else if pct > 80 {
            // Red circle for near-full context
            color(C_RED, "◉", self.color)
        } else if pct > 50 {
            // Yellow circle for warning
            color(C_YELLOW, "◉", self.color)
        } else {
            // Green circle for nominal
            color(C_GREEN, "●", self.color)
        }
    }

    // ─── Render: Compact single-line ────────────────────────────────────────

    /// Compact single-line status bar. Never wraps, uses abbreviated segments.
    /// Fits in terminals as narrow as 60 chars.
    ///
    /// Layout:
    ///   ◆ model ▎ctx 45% ▏$0.23 ▏14m ▏MCP:3 ▏Sub:2/5 ▏T:7/2f ▏safe
    pub fn render(&self) -> String {
        let elapsed = elapsed_str(self.start.elapsed());
        let ctx = self.context_pct();
        let sep = dim(SEP, self.color);
        let model_c = color(C_CYAN, &self.model, self.color);

        let mut parts: Vec<String> = vec![];

        // Indicator + model
        parts.push(format!("{} {}", self.indicator(), model_c));

        // Context %
        parts.push(format!("ctx {}", self.ctx_colored(ctx)));

        // Cost
        parts.push(self.cost_segment());

        // Elapsed
        parts.push(color(C_GRAY, &elapsed, self.color));

        // MCP (if any)
        if self.mcp_servers_configured > 0 {
            let mcp_str = if self.mcp_servers_connected == self.mcp_servers_configured {
                format!("MCP:{}", self.mcp_servers_configured)
            } else {
                format!("MCP:{}/{}", self.mcp_servers_connected, self.mcp_servers_configured)
            };
            parts.push(color(C_CYAN, &mcp_str, self.color));
        }

        // Sub-agents (if any)
        if self.total_subagents() > 0 {
            let sa = format!("Sub:{}/{}", self.subagent_active, self.total_subagents());
            let sa_colored = if self.subagent_failed > 0 {
                color(C_RED, &sa, self.color)
            } else {
                color(C_PURPLE, &sa, self.color)
            };
            parts.push(sa_colored);
        }

        // Tool calls (if any)
        if self.total_tool_calls() > 0 {
            let tc = format!("T:{}/{}", self.tool_calls_ok, self.tool_calls_failed);
            parts.push(self.tool_color(&tc));
        }

        // Hook activity (if any)
        if self.hooks_fired > 0 {
            let hk = format!("H:{}", self.hooks_fired);
            parts.push(color(C_GRAY, &hk, self.color));
        }

        // HITL risk level
        parts.push(self.risk_colored());

        parts.join(&format!(" {} ", sep))
    }

    // ─── Render: Detailed two-line ──────────────────────────────────────────

    /// Two-line detailed status bar. Row 1 has core metrics + rate. Row 2 has
    /// tools breakdown, sub-agents, memory, hooks, HITL, budget.
    ///
    /// Row 1:
    ///   ◆ model ▎ctx ━━━━━━━━░░ 45% ▏$0.23 ▏14m ▏⚡0.7 t/s ▏H:3
    /// Row 2:
    ///   Tools:7ok│2fail│1blk│ Mem:42│ Sub:2a/5d/1f│ Todo:3/2│ Budget:62%│ ●safe
    pub fn render_detailed(&self) -> (String, String) {
        let elapsed = elapsed_str(self.start.elapsed());
        let ctx = self.context_pct();
        let sep = dim(SEP, self.color);

        // ── Row 1: core session ─────────────────────────────────────────────
        let r1_indicator = format!("{}", self.indicator());
        let r1_model = color(C_CYAN, &self.model, self.color);
        let r1_bar = self.context_bar();
        let r1_ctx = self.ctx_colored(ctx);
        let r1_cost = self.cost_segment();
        let r1_elapsed = color(C_GRAY, &elapsed, self.color);

        let mut r1_parts = vec![
            format!("{} {}", r1_indicator, r1_model),
            format!("ctx {} {}%", r1_bar, r1_ctx),
            r1_cost,
            r1_elapsed,
        ];

        // Turns per second (if >0)
        let tps = self.turns_per_sec();
        if tps > 0.0 {
            r1_parts.push(color(C_BLUE, &format!("⚡{:.1}t/s", tps), self.color));
        }

        // Hook count
        if self.hooks_fired > 0 {
            r1_parts.push(color(C_GRAY, &format!("H:{}", self.hooks_fired), self.color));
        }

        let row1 = r1_parts.join(&format!(" {} ", sep));

        // ── Row 2: detail ───────────────────────────────────────────────────
        let mut r2_parts: Vec<String> = vec![];

        // Tool calls breakdown
        if self.total_tool_calls() > 0 {
            let mut t_parts: Vec<String> = vec!["Tools:".to_string()];
            t_parts.push(format!("{}ok", self.tool_calls_ok));
            if self.tool_calls_failed > 0 {
                t_parts.push(color(C_RED, &format!("{}fail", self.tool_calls_failed), self.color));
            } else {
                t_parts.push("0fail".to_string());
            }
            if self.tool_calls_blocked > 0 {
                t_parts.push(color(C_YELLOW, &format!("{}blk", self.tool_calls_blocked), self.color));
            }
            r2_parts.push(t_parts.join("│"));
        }

        // Memory
        if self.memory_entries > 0 {
            r2_parts.push(color(C_CYAN, &format!("Mem:{}", self.memory_entries), self.color));
        }

        // Sub-agents
        if self.total_subagents() > 0 {
            let sa = format!("Sub:{}a/{}d/{}f", self.subagent_active, self.subagent_completed, self.subagent_failed);
            let sa_colored = if self.subagent_failed > 0 {
                color(C_RED, &sa, self.color)
            } else {
                color(C_PURPLE, &sa, self.color)
            };
            r2_parts.push(sa_colored);
        }

        // Todo
        if self.todo_active > 0 || self.todo_done > 0 {
            r2_parts.push(format!(
                "Todo:{}/{}",
                color(C_YELLOW, &self.todo_active.to_string(), self.color),
                self.todo_done,
            ));
        }

        // Budget
        if self.max_cost > 0.0 && self.max_cost < f64::MAX {
            let bp = self.budget_pct();
            r2_parts.push(format!(
                "Budget:{}",
                color(self.budget_color(), &format!("{:.0}%", bp), self.color),
            ));
        }

        // HITL
        r2_parts.push(format!(
            "{}",
            self.risk_colored()
        ));

        let row2 = r2_parts.join("│");

        (row1, row2)
    }

    // ─── Color helpers ──────────────────────────────────────────────────────

    fn ctx_colored(&self, pct: u32) -> String {
        color(self.context_color(), &format!("{}%", pct), self.color)
    }

    fn cost_segment(&self) -> String {
        color(C_YELLOW, &format!("${:.2}", self.cost), self.color)
    }

    fn tool_color(&self, text: &str) -> String {
        color(self.failure_color(), text, self.color)
    }

    fn risk_colored(&self) -> String {
        let (c, indicator) = match self.risk_level.as_str() {
            "critical" | "high" => (C_RED, "●"),
            "medium" => (C_YELLOW, "◉"),
            "low" => (C_BLUE, "●"),
            _ => (C_GREEN, "●"),
        };
        color(c, &format!("{}{}", indicator, self.risk_level), self.color)
    }
}

// ─── Elapsed Time Formatting ────────────────────────────────────────────────

/// Format a Duration into a human-readable elapsed string.
fn elapsed_str(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a bar with color disabled for clean test assertions.
    fn bar_no_color() -> StatusBar {
        let mut b = StatusBar::new(Some("claude-sonnet-4"));
        b.set_color(false);
        b
    }

    #[test]
    fn test_status_bar_initial_state() {
        let b = bar_no_color();
        let line = b.render();
        assert!(line.contains("claude-sonnet-4"), "should show model name");
        assert!(line.contains("ctx"), "should show context");
        assert!(line.contains("$0.00"), "should show zero cost");
        assert!(line.contains("safe"), "should show risk level");
        assert!(!line.contains("MCP:"), "no MCP when zero");
        assert!(!line.contains("Sub:"), "no sub-agents when zero");
        assert!(!line.contains("T:"), "no tool calls when zero");
    }

    #[test]
    fn test_status_bar_tracks_tool_calls() {
        let mut b = bar_no_color();
        for _ in 0..7 { b.track_tool_ok(); }
        for _ in 0..2 { b.track_tool_failure(); }
        let line = b.render();
        assert!(line.contains("T:7/2"), "should show ok/fail counts");
    }

    #[test]
    fn test_status_bar_tracks_subagents() {
        let mut b = bar_no_color();
        b.set_subagent_stats(2, 5, 1);
        let line = b.render();
        assert!(line.contains("Sub:2/8"), "2 active / 8 total");
    }

    #[test]
    fn test_status_bar_tracks_hooks() {
        let mut b = bar_no_color();
        b.track_hook_fired();
        b.track_hook_blocked();
        let line = b.render();
        assert!(line.contains("H:2"), "should show 2 hooks fired");
    }

    #[test]
    fn test_status_bar_tracks_mcp() {
        let mut b = bar_no_color();
        b.set_mcp_servers(3, 3);
        let line = b.render();
        assert!(line.contains("MCP:3"), "all connected");
    }

    #[test]
    fn test_status_bar_mcp_partial() {
        let mut b = bar_no_color();
        b.set_mcp_servers(3, 2);
        let line = b.render();
        assert!(line.contains("MCP:2/3"), "2/3 connected");
    }

    #[test]
    fn test_status_bar_tracks_memory() {
        let mut b = bar_no_color();
        b.set_memory_entries(42);
        let line = b.render();
        // Memory only shows in detailed view
        assert!(!line.contains("Mem:"), "memory hidden in compact");
    }

    #[test]
    fn test_detailed_shows_memory() {
        let mut b = bar_no_color();
        b.set_memory_entries(42);
        let (r1, r2) = b.render_detailed();
        assert!(r1.contains("claude-sonnet-4"), "row1 shows model");
        assert!(r2.contains("Mem:42"), "row2 shows memory");
    }

    #[test]
    fn test_detailed_tool_breakdown() {
        let mut b = bar_no_color();
        for _ in 0..7 { b.track_tool_ok(); }
        b.track_tool_failure();
        let (_, r2) = b.render_detailed();
        assert!(r2.contains("Tools:"), "row2 has tool section");
        assert!(r2.contains("7ok"), "7 ok calls");
        assert!(r2.contains("1fail"), "1 failure");
    }

    #[test]
    fn test_detailed_todo() {
        let mut b = bar_no_color();
        b.set_todo_counts(3, 5);
        let (_, r2) = b.render_detailed();
        assert!(r2.contains("Todo:3/5"), "todo numbers");
    }

    #[test]
    fn test_detailed_budget() {
        let mut b = bar_no_color();
        b.set_max_cost(10.0);
        b.add_tokens(1000, 500, 7.50);
        let (_, r2) = b.render_detailed();
        assert!(r2.contains("Budget:75%"), "75% of $10 used");
    }

    #[test]
    fn test_turns_per_sec_renders() {
        let b = bar_no_color();
        // No time has elapsed, so no tps
        let (r1, _) = b.render_detailed();
        assert!(!r1.contains("t/s"), "no rate when no turns");
    }

    #[test]
    fn test_context_bar_render() {
        let mut b = bar_no_color();
        b.set_max_context(1000);
        b.add_tokens(500, 0, 0.0);
        assert_eq!(b.context_pct(), 50);
    }

    #[test]
    fn test_risk_level_coloring() {
        let mut b = bar_no_color();
        b.set_risk_level("critical");
        let line = b.render();
        assert!(line.contains("critical"));
    }

    #[test]
    fn test_indicator_color_logic() {
        let mut b = bar_no_color();
        // No failures, low context → green indicator character
        assert!(b.indicator().contains("●"));

        // Failures → diamond
        b.track_tool_failure();
        assert!(b.indicator().contains("◆"));
    }

    #[test]
    fn test_elapsed_str() {
        assert_eq!(elapsed_str(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(elapsed_str(std::time::Duration::from_secs(90)), "1m 30s");
        assert_eq!(elapsed_str(std::time::Duration::from_secs(3661)), "1h 1m");
    }

    #[test]
    fn test_context_pct_clamped() {
        let mut b = bar_no_color();
        b.set_max_context(1000);
        b.add_tokens(9999, 0, 0.0);
        assert_eq!(b.context_pct(), 100, "should clamp at 100%");
    }

    #[test]
    fn test_budget_pct_overflow() {
        let mut b = bar_no_color();
        b.set_max_cost(10.0);
        b.add_tokens(0, 0, 20.0);
        assert!((b.budget_pct() - 100.0).abs() < 0.01, "should clamp at 100%");
    }

    #[test]
    fn test_banner_renders() {
        let banner = render_banner("0.1.0", Some("test"), 3, 5, 12);
        assert!(banner.contains("0.1.0"));
        assert!(banner.contains("mcp"));
        assert!(banner.contains("agents"));
        assert!(banner.len() > 100, "banner should be substantial");
    }

    #[test]
    fn test_color_enabled() {
        let mut b = StatusBar::new(Some("test-model"));
        b.set_color(true);
        let line = b.render();
        assert!(line.contains("\x1b["), "color codes present when enabled");
    }

    #[test]
    fn test_color_disabled() {
        let mut b = StatusBar::new(Some("test-model"));
        b.set_color(false);
        let line = b.render();
        assert!(!line.contains("\x1b["), "no color codes when disabled");
    }

    #[test]
    fn test_render_with_all_features() {
        let mut b = bar_no_color();
        b.set_model("claude-sonnet-4-5");
        b.set_max_context(200_000);
        b.add_tokens(45000, 12000, 0.85);
        for _ in 0..12 { b.track_tool_ok(); }
        b.track_tool_failure();
        b.track_tool_blocked();
        b.set_subagent_stats(1, 3, 0);
        b.set_mcp_servers(4, 4);
        b.set_memory_entries(56);
        b.set_risk_level("low");
        b.set_todo_counts(2, 8);
        b.track_hook_fired();
        b.add_turn();
        b.add_turn();

        let line = b.render();
        assert!(line.contains("claude-sonnet-4-5"));
        assert!(line.contains("T:12/1"));
        assert!(line.contains("Sub:1/4"));
        assert!(line.contains("MCP:4"));
        assert!(line.contains("low"));

        let (r1, r2) = b.render_detailed();
        assert!(r1.contains("14m") || r1.contains("0s"), "elapsed time");
        assert!(r2.contains("Mem:56"), "memory in detailed");
        assert!(r2.contains("Todo:2/8"), "todo in detailed");
    }
}
