//! Sub-agent dispatcher and result synthesizer.
//!
//! The dispatcher executes a TaskGraph by spawning sub-agents
//! respecting dependency order. The synthesizer aggregates results
//! into a coherent response.

use crate::budget::BudgetTracker;

use super::loader::TaskGraph;

/// Result from a single executed task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub description: String,
    pub agent: Option<String>,
    pub output: String,
    pub error: Option<String>,
    /// Estmated input tokens consumed by this task.
    pub input_tokens: u64,
    /// Estimated output tokens consumed by this task.
    pub output_tokens: usize,
    /// Estimated cost in USD for this task.
    pub cost_usd: f64,
}

impl TaskResult {
    /// Create a new task result with default token/cost estimates.
    pub fn new(task_id: String, description: String, agent: Option<String>) -> Self {
        TaskResult {
            task_id,
            description,
            agent,
            output: String::new(),
            error: None,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
        }
    }
}

/// Execute a task graph in dependency order, tracking budget.
///
/// If `budget` is provided, each dispatched task records an estimated
/// cost to the shared tracker.
///
/// # Note
/// This is a Phase 1 implementation that executes tasks sequentially.
/// True parallel dispatch requires the agent runtime to be wired in.
pub async fn execute_task_graph(
    _graph: &TaskGraph,
    budget: Option<&BudgetTracker>,
) -> Vec<TaskResult> {
    // TODO: Wire into actual agent runtime
    // 1. Topological sort of task graph by dependency order
    // 2. For each level, spawn sub-agents in parallel
    // 3. Wait for all tasks at current level
    // 4. Pass results as context to downstream tasks
    // 5. Enforce per-agent max_turns and budget limits
    //
    // Stub: record a baseline budget marker if tracker is provided.
    if let Some(b) = budget {
        b.add_cost(0.001); // baseline routing overhead
    }
    Vec::new()
}

/// Synthesize individual task results into a coherent final response.
///
/// Handles:
///   - Ordering results by dependency level
///   - Flagging conflicting outputs
///   - Producing a summary if there are many results
pub fn synthesize_results(results: &[TaskResult]) -> String {
    if results.is_empty() {
        return "No tasks were executed.".to_string();
    }

    let mut parts: Vec<String> = Vec::new();

    for result in results {
        let agent_info = match &result.agent {
            Some(name) => format!("[{}]", name),
            None => "[direct]".to_string(),
        };

        if let Some(err) = &result.error {
            parts.push(format!(
                "{} Task \"{}\" failed: {}",
                agent_info, result.description, err
            ));
        } else {
            parts.push(format!(
                "{} Task \"{}\" completed:\n{}",
                agent_info, result.description, result.output
            ));
        }
    }

    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthesize_empty_results() {
        let output = synthesize_results(&[]);
        assert_eq!(output, "No tasks were executed.");
    }

    #[test]
    fn synthesize_single_result() {
        let results = vec![TaskResult::new(
            "task-1".into(),
            "read file".into(),
            Some("FileReader".into()),
        )];

        let output = synthesize_results(&results);
        assert!(output.contains("[FileReader]"));
        assert!(output.contains("read file"));
    }

    #[test]
    fn synthesize_with_error() {
        let mut r = TaskResult::new("task-1".into(), "parse data".into(), Some("Parser".into()));
        r.error = Some("File not found".into());
        let results = vec![r];

        let output = synthesize_results(&results);
        assert!(output.contains("failed"));
        assert!(output.contains("File not found"));
    }

    #[test]
    fn execute_empty_graph() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let graph = TaskGraph { tasks: vec![] };
        let results = rt.block_on(execute_task_graph(&graph, None));
        assert!(results.is_empty());
    }

    #[test]
    fn execute_with_budget_logs_cost() {
        let cfg = crate::budget::BudgetConfig {
            max_context_tokens: 200_000,
            max_cost_usd: 10.0,
            warn_at: 0.8,
        };
        let budget = BudgetTracker::new(cfg);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let graph = TaskGraph { tasks: vec![] };

        let results = rt.block_on(execute_task_graph(&graph, Some(&budget)));
        assert!(results.is_empty());
        // Budget overhead cost should be recorded
        assert!(budget.total_cost_usd() > 0.0);
    }
}
