/// Startup ASCII banner for Ante.
///
/// Renders a large geometric "ANTE" logo with version info,
/// extensibility features summary, and connection stats.
use std::time::Instant;

/// Render the startup banner to a string.
pub fn render_banner(
    version: &str,
    model: Option<&str>,
    mcp_count: usize,
    agent_count: usize,
    memory_count: usize,
) -> String {
    let mut s = String::new();

    // ── Logo ────────────────────────────────────────────────────────────
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

    // ── Feature grid ────────────────────────────────────────────────────
    s.push_str("   ═══════════════════════════════════════════════════\n");
    s.push_str(&format!("   v{:<47}\n", version));

    if let Some(model) = model {
        s.push_str(&format!("   model │ {:<44}\n", model));
    }

    s.push_str("   ─────────────────────────────────────────────────\n");

    // Feature badges
    let badge = |name: &str, count: usize, enabled: bool| -> String {
        if enabled {
            format!("[{}] {} {} ", "✓", name, if count > 0 { count } else { 0 })
        } else {
            format!("[ ] {} - ", name)
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

/// A compact status bar line showing session key metrics.
///
/// Format (one line, updated in-place):
///   ▼ claude-sonnet-4 │ ctx:█ 45% │ $0.23 │ 14m │ MCP:2 Mem:12 ▲
///
pub struct StatusBar {
    start: Instant,
    model: String,
    token_prompt: u64,
    token_completion: u64,
    max_context: u64,
    cost: f64,
    mcp_servers: usize,
    memory_entries: usize,
    agents_loaded: usize,
    risk_level: String,
    turn_count: u32,
    tool_calls: u32,
}

impl StatusBar {
    pub fn new(model: Option<&str>) -> Self {
        StatusBar {
            start: Instant::now(),
            model: model.unwrap_or("—").to_string(),
            token_prompt: 0,
            token_completion: 0,
            max_context: 200_000,
            cost: 0.0,
            mcp_servers: 0,
            memory_entries: 0,
            agents_loaded: 0,
            risk_level: "safe".into(),
            turn_count: 0,
            tool_calls: 0,
        }
    }

    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
    }

    pub fn add_tokens(&mut self, prompt: u64, completion: u64, cost: f64) {
        self.token_prompt += prompt;
        self.token_completion += completion;
        self.cost += cost;
    }

    pub fn add_turn(&mut self) {
        self.turn_count += 1;
    }

    pub fn add_tool_call(&mut self) {
        self.tool_calls += 1;
    }

    pub fn set_mcp_servers(&mut self, n: usize) {
        self.mcp_servers = n;
    }

    pub fn set_memory_entries(&mut self, n: usize) {
        self.memory_entries = n;
    }

    pub fn set_agents(&mut self, n: usize) {
        self.agents_loaded = n;
    }

    pub fn set_risk_level(&mut self, level: &str) {
        self.risk_level = level.to_string();
    }

    /// Render a single-line status bar string.
    ///
    /// Examples:
    ///   ◉ claude-sonnet-4  ▏ctx 45%  ▏$0.23  ▏14m  ▏MCP:2  ▏Mem:12  ▏t:4  ▏c:7
    ///   ◉ claude-sonnet-4  ▏ctx ████████░░ 45%  ▏14m  ▏MCP:2  ▏tools:7
    pub fn render(&self) -> String {
        let elapsed = elapsed_str(self.start.elapsed());
        let total = self.token_prompt + self.token_completion;
        let pct = if self.max_context > 0 {
            (total as f64 / self.max_context as f64 * 100.0) as u32
        } else {
            0
        };

        // Context bar: ████████░░
        let bar_width = 10usize;
        let filled = ((pct as f64 / 100.0) * bar_width as f64).round() as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width - filled;
        let bar: String = std::iter::repeat('█').take(filled)
            .chain(std::iter::repeat('░').take(empty))
            .collect();

        // Color indicator based on context usage
        let indicator = if pct > 80 {
            "◉" // red/critical — but plain ASCII
        } else if pct > 50 {
            "◉" // yellow/warning
        } else {
            "●" // green/normal
        };

        format!(
            "{} {}  ▏ctx {} {}%  ▏${:.2}  ▏{}  ▏MCP:{}  ▏Mem:{}  ▏Ag:{}  ▏{}",
            indicator,
            self.model,
            bar,
            pct,
            self.cost,
            elapsed,
            self.mcp_servers,
            self.memory_entries,
            self.agents_loaded,
            self.risk_level,
        )
    }

    /// Render a 2-row status for terminals that support it (like gemini-cli).
    /// Row 1: model + context bar + cost + elapsed
    /// Row 2: MCP + memory + agents + HITL risk level + tool calls
    pub fn render_double(&self) -> (String, String) {
        let elapsed = elapsed_str(self.start.elapsed());
        let total = self.token_prompt + self.token_completion;
        let pct = if self.max_context > 0 {
            (total as f64 / self.max_context as f64 * 100.0) as u32
        } else {
            0
        };

        let bar_width = 10usize;
        let filled = ((pct as f64 / 100.0) * bar_width as f64).round() as usize;
        let filled = filled.min(bar_width);
        let empty = bar_width - filled;
        let bar: String = std::iter::repeat('█').take(filled)
            .chain(std::iter::repeat('░').take(empty))
            .collect();

        let indicator = if pct > 80 { "◉" } else if pct > 50 { "◉" } else { "●" };

        let row1 = format!(
            "{} {}  ▏ctx {} {}%  ▏${:.2}  ▏{}",
            indicator, self.model, bar, pct, self.cost, elapsed,
        );

        let row2 = format!(
            " ▏MCP:{}  ▏Mem:{}  ▏Ag:{}  ▏turns:{}  ▏tools:{}  ▏{}",
            self.mcp_servers, self.memory_entries, self.agents_loaded,
            self.turn_count, self.tool_calls, self.risk_level,
        );

        (row1, row2)
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_render() {
        let bar = StatusBar::new(Some("claude-sonnet-4"));
        let line = bar.render();
        assert!(line.contains("claude-sonnet-4"));
        assert!(line.contains("ctx"));
        assert!(line.contains("MCP:0"));
        assert!(line.contains("safe"));
    }

    #[test]
    fn test_status_bar_updates() {
        let mut bar = StatusBar::new(None);
        bar.add_tokens(1000, 200, 0.05);
        bar.add_turn();
        bar.add_tool_call();
        bar.set_mcp_servers(3);
        bar.set_memory_entries(42);
        let line = bar.render();
        assert!(line.contains("MCP:3"));
        assert!(line.contains("Mem:42"));
        assert!(line.contains("$0.05"));
    }

    #[test]
    fn test_double_row() {
        let mut bar = StatusBar::new(Some("test-model"));
        bar.add_tokens(50000, 10000, 0.50);
        bar.add_turn();
        bar.add_tool_call();
        bar.set_mcp_servers(2);
        let (r1, r2) = bar.render_double();
        assert!(r1.contains("test-model"));
        assert!(r1.contains("ctx"));
        assert!(r2.contains("MCP:2"));
        assert!(r2.contains("turns:1"));
        assert!(r2.contains("tools:1"));
    }

    #[test]
    fn test_elapsed_str() {
        assert_eq!(elapsed_str(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(elapsed_str(std::time::Duration::from_secs(90)), "1m 30s");
        assert_eq!(elapsed_str(std::time::Duration::from_secs(3661)), "1h 1m");
    }

    #[test]
    fn test_context_bar_filled() {
        let mut bar = StatusBar::new(None);
        bar.max_context = 100_000;
        bar.add_tokens(80_000, 0, 0.0);
        let line = bar.render();
        // 80% = 8/10 blocks
        assert!(line.contains("████████░░"));
    }

    #[test]
    fn test_banner_renders() {
        let banner = render_banner("0.1.0", Some("test"), 3, 5, 12);
        assert!(banner.contains("0.1.0"));
        assert!(banner.contains("mcp"));
        assert!(banner.contains("agents"));
        assert!(banner.len() > 100, "banner should be substantial");
    }
}
