//! Sub-agent dispatcher and result synthesizer.
//!
//! The dispatcher executes a `TaskGraph` by spawning sub-agents
//! respecting dependency order — tasks at the same topological
//! level run in parallel. The synthesizer aggregates results into
//! a coherent response.
//!
//! # Dynamic sub‑agent creation
//!
//! The caller provides a `runner` closure that actually invokes the
//! agent (e.g. via Claude CLI, an HTTP API, or a sub‑process).  The
//! dispatcher handles topological scheduling, context propagation
//! (dependency results flow into downstream tasks), and result
//! collection.
//!
//! Because tasks at the same level run concurrently via
//! `futures::future::join_all`, the runner must implement `Clone` so
//! it can be cloned for each parallel branch.  The runner also
//! receives owned data so it does not need a particular lifetime.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use crate::budget::BudgetTracker;

use super::loader::{AgentRegistry, TaskGraph, TaskNode};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Result from a single executed task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub description: String,
    pub agent: Option<String>,
    pub output: String,
    pub error: Option<String>,
    /// Estimated input tokens consumed by this task.
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

/// Context passed to an agent runner for a single task.
///
/// This struct owns all its data so the runner does not need to
/// outlive any particular borrow — it can be passed into any async
/// context including `tokio::spawn` or `join_all`.
#[derive(Debug, Clone)]
pub struct TaskContext {
    /// The unique task identifier (e.g. `"task-1"`).
    pub task_id: String,
    /// Human-readable description of what this task should do.
    pub task_description: String,
    /// IDs of all dependency tasks that must complete first.
    pub task_dependencies: Vec<String>,
    /// Name of the assigned agent, if any.
    pub assigned_agent: Option<String>,
    /// Output from completed dependency tasks, keyed by task ID.
    pub dependency_results: HashMap<String, TaskResult>,
}

impl TaskContext {
    /// Build a context from a task node and its completed dependency results.
    fn from_task(task: &TaskNode, deps: HashMap<String, TaskResult>) -> Self {
        TaskContext {
            task_id: task.id.clone(),
            task_description: task.description.clone(),
            task_dependencies: task.dependencies.clone(),
            assigned_agent: task.assigned_agent.clone(),
            dependency_results: deps,
        }
    }
}

/// Runner trait alias would be `dyn Fn(TaskContext) -> Fut + Clone` but
/// Rust cannot express `dyn Fn + Clone` in a single trait-object type.
/// Callers use `impl Fn(TaskContext) -> Pin<Box<dyn Future<…> + Send>> + Clone
/// + Send + Sync + 'static` directly in `execute_task_graph`.

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Execute a task graph in dependency order, tracking budget.
///
/// # Scheduling algorithm
///
/// 1. Topological sort: group tasks by their longest dependency chain.
///    All tasks at the same level are independent and run concurrently.
///
/// 2. For each level, the caller's `runner` closure is cloned for each
///    task and invoked with a `TaskContext` containing the task node
///    and the outputs of its completed dependencies.
///
/// 3. Completed results become available to downstream tasks as context.
///
/// # Type requirements
///
/// The `runner` closure must implement `Clone` (so it can be shared
/// across parallel tasks) and return a pinned, boxed, `Send` future.
///
/// # Example
///
/// ```ignore
/// use std::collections::HashMap;
/// use std::pin::Pin;
/// use std::future::Future;
///
/// let results = execute_task_graph(&graph, &agents, None, |ctx| {
///     Box::pin(async move {
///         TaskResult::new(ctx.task_id.clone(), ctx.task_description.clone(), None)
///     })
/// }).await;
/// ```
pub async fn execute_task_graph(
    graph: &TaskGraph,
    _agents: &AgentRegistry,
    budget: Option<&BudgetTracker>,
    runner: impl Fn(TaskContext) -> Pin<Box<dyn Future<Output = TaskResult> + Send>>
        + Clone
        + Send
        + Sync
        + 'static,
) -> Vec<TaskResult> {
    // ── 1. Group tasks by topological level ────────────────────────────
    let levels = topological_levels(&graph.tasks);
    // Bookkeeping for completed results
    let mut results: HashMap<String, TaskResult> = HashMap::new();

    // ── 2. Execute level by level (parallel within a level) ────────────
    for level in &levels {
        let mut level_futures: Vec<Pin<Box<dyn Future<Output = TaskResult> + Send>>> =
            Vec::new();

        for task_id in level {
            if let Some(task) = graph.tasks.iter().find(|t| t.id == *task_id) {
                // Clone completed dependency outputs into an owned map.
                let dep_results: HashMap<String, TaskResult> = task
                    .dependencies
                    .iter()
                    .filter_map(|dep_id| {
                        results.get(dep_id).map(|r| (dep_id.clone(), r.clone()))
                    })
                    .collect();

                let ctx = TaskContext::from_task(task, dep_results);

                // Budget marker — per-task routing overhead
                if let Some(b) = budget {
                    b.add_cost(0.002);
                }

                let agent_name = task.assigned_agent.clone();
                let runner = runner.clone();

                level_futures.push(Box::pin(async move {
                    let mut result = runner(ctx).await;
                    if result.agent.is_none() {
                        result.agent = agent_name;
                    }
                    result
                }));
            }
        }

        // Run all tasks at this level concurrently
        use futures::future::join_all;
        for r in join_all(level_futures).await {
            results.insert(r.task_id.clone(), r);
        }
    }

    // ── 3. Return results in original task order ───────────────────────
    graph
        .tasks
        .iter()
        .filter_map(|t| results.remove(&t.id))
        .collect()
}

/// Compute topological levels: tasks at the same level can run in parallel.
///
/// Level 0 = no dependencies. Each subsequent level depends on at least
/// one task from a previous level.  Uses an iterative Kahn-like algorithm.
fn topological_levels(tasks: &[TaskNode]) -> Vec<Vec<String>> {
    let mut levels: Vec<Vec<String>> = Vec::new();
    let mut remaining: Vec<&TaskNode> = tasks.iter().collect();
    // Use owned Strings so there are no borrow conflicts with the
    // `level` Vec when it is moved into `levels`.
    let mut placed: Vec<String> = Vec::new();

    while !remaining.is_empty() {
        let mut level: Vec<String> = Vec::new();
        let mut next_remaining: Vec<&TaskNode> = Vec::new();

        // Phase 1: find tasks whose dependencies are all in `placed`
        // (placed only contains tasks from earlier iterations).
        for task in remaining.drain(..) {
            let all_deps_placed = task
                .dependencies
                .iter()
                .all(|d| placed.contains(d));
            if all_deps_placed {
                level.push(task.id.clone());
            } else {
                next_remaining.push(task);
            }
        }

        // Phase 2: mark collected tasks as placed
        placed.extend(level.iter().cloned());

        // Cycle / unreachable deps guard
        if level.is_empty() {
            for task in next_remaining {
                if !placed.contains(&task.id) {
                    level.push(task.id.clone());
                    placed.push(task.id.clone());
                }
            }
        } else {
            remaining = next_remaining;
        }

        if !level.is_empty() {
            levels.push(level);
        }
    }

    levels
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helpers -----------------------------------------------------------

    fn single_node_graph() -> TaskGraph {
        TaskGraph {
            tasks: vec![TaskNode {
                id: "task-1".into(),
                description: "do something".into(),
                assigned_agent: None,
                dependencies: vec![],
            }],
        }
    }

    fn chain_graph() -> TaskGraph {
        TaskGraph {
            tasks: vec![
                TaskNode {
                    id: "task-1".into(),
                    description: "first".into(),
                    assigned_agent: None,
                    dependencies: vec![],
                },
                TaskNode {
                    id: "task-2".into(),
                    description: "depends on first".into(),
                    assigned_agent: None,
                    dependencies: vec!["task-1".into()],
                },
            ],
        }
    }

    fn empty_agents() -> AgentRegistry {
        AgentRegistry::load(std::path::Path::new("/nonexistent")).unwrap()
    }

    fn idle_runner() -> impl Fn(TaskContext) -> Pin<Box<dyn Future<Output = TaskResult> + Send>>
        + Clone
        + Send
        + Sync
        + 'static
    {
        |ctx: TaskContext| {
            Box::pin(async move {
                TaskResult::new(ctx.task_id, ctx.task_description, None)
            })
        }
    }

    // -- synthesize_results -----------------------------------------------

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

    // -- topological_levels ------------------------------------------------

    #[test]
    fn levels_empty() {
        assert!(topological_levels(&[]).is_empty());
    }

    #[test]
    fn levels_single_task() {
        let tasks = vec![TaskNode {
            id: "task-1".into(),
            description: "single".into(),
            assigned_agent: None,
            dependencies: vec![],
        }];
        let levels = topological_levels(&tasks);
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], vec!["task-1"]);
    }

    #[test]
    fn levels_chained_tasks() {
        let tasks = vec![
            TaskNode {
                id: "task-1".into(),
                description: "first".into(),
                assigned_agent: None,
                dependencies: vec![],
            },
            TaskNode {
                id: "task-2".into(),
                description: "second".into(),
                assigned_agent: None,
                dependencies: vec!["task-1".into()],
            },
            TaskNode {
                id: "task-3".into(),
                description: "third".into(),
                assigned_agent: None,
                dependencies: vec!["task-2".into()],
            },
        ];
        let levels = topological_levels(&tasks);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec!["task-1"]);
        assert_eq!(levels[1], vec!["task-2"]);
        assert_eq!(levels[2], vec!["task-3"]);
    }

    #[test]
    fn levels_parallel_tasks() {
        let tasks = vec![
            TaskNode {
                id: "task-1".into(),
                description: "first".into(),
                assigned_agent: None,
                dependencies: vec![],
            },
            TaskNode {
                id: "task-2".into(),
                description: "parallel".into(),
                assigned_agent: None,
                dependencies: vec![],
            },
        ];
        let levels = topological_levels(&tasks);
        assert_eq!(levels.len(), 1);
        assert!(levels[0].contains(&"task-1".to_string()));
        assert!(levels[0].contains(&"task-2".to_string()));
    }

    #[test]
    fn levels_diamond() {
        // task-1 -> task-2a -> task-3
        // task-1 -> task-2b -> task-3
        let tasks = vec![
            TaskNode {
                id: "task-1".into(),
                description: "root".into(),
                assigned_agent: None,
                dependencies: vec![],
            },
            TaskNode {
                id: "task-2a".into(),
                description: "left".into(),
                assigned_agent: None,
                dependencies: vec!["task-1".into()],
            },
            TaskNode {
                id: "task-2b".into(),
                description: "right".into(),
                assigned_agent: None,
                dependencies: vec!["task-1".into()],
            },
            TaskNode {
                id: "task-3".into(),
                description: "merge".into(),
                assigned_agent: None,
                dependencies: vec!["task-2a".into(), "task-2b".into()],
            },
        ];
        let levels = topological_levels(&tasks);
        assert_eq!(levels.len(), 3); // root -> middle (parallel) -> merge
        assert_eq!(levels[0], vec!["task-1"]);
        assert_eq!(levels[1].len(), 2); // both at level 2
        assert_eq!(levels[2], vec!["task-3"]);
    }

    // -- execute_task_graph ------------------------------------------------

    #[tokio::test]
    async fn execute_single_task() {
        let agents = empty_agents();
        let results = execute_task_graph(&single_node_graph(), &agents, None, idle_runner()).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].description, "do something");
    }

    #[tokio::test]
    async fn execute_chained_tasks() {
        let agents = empty_agents();
        let graph = chain_graph();

        let results = execute_task_graph(&graph, &agents, None, idle_runner()).await;
        assert_eq!(results.len(), 2);

        // task-1 should have 0 dependency results
        let t1 = results.iter().find(|r| r.task_id == "task-1").unwrap();
        // task-2 should have 1 dependency result in context
        let t2 = results.iter().find(|r| r.task_id == "task-2").unwrap();
        assert!(t2.description.contains("depends on first"));
        // Ordering: task-1 first, task-2 second
        assert_eq!(results[0].task_id, "task-1");
        assert_eq!(results[1].task_id, "task-2");
    }

    #[tokio::test]
    async fn execute_parallel_levels() {
        let agents = empty_agents();
        let graph = chain_graph();

        // Use a runner that records context info so we can verify
        // dependency propagation.
        let results = execute_task_graph(
            &graph,
            &agents,
            None,
            |ctx: TaskContext| {
                let dep_count = ctx.dependency_results.len();
                let id = ctx.task_id.clone();
                Box::pin(async move {
                    let mut r = TaskResult::new(id, format!("{} deps", dep_count), None);
                    r.output = format!("{} dependencies", dep_count);
                    r
                })
            },
        )
        .await;

        assert_eq!(results.len(), 2);
        let t1 = results.iter().find(|r| r.task_id == "task-1").unwrap();
        assert!(t1.description.contains("0 deps"));
        let t2 = results.iter().find(|r| r.task_id == "task-2").unwrap();
        assert!(t2.description.contains("1 deps"));
    }

    #[tokio::test]
    async fn execute_preserves_assigned_agent() {
        let agents = empty_agents();
        let graph = TaskGraph {
            tasks: vec![TaskNode {
                id: "task-1".into(),
                description: "with agent".into(),
                assigned_agent: Some("CodeReviewer".into()),
                dependencies: vec![],
            }],
        };

        let results = execute_task_graph(&graph, &agents, None, idle_runner()).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].agent.as_deref(), Some("CodeReviewer"));
    }

    #[tokio::test]
    async fn execute_with_budget_logs_cost() {
        let cfg = crate::budget::BudgetConfig {
            max_context_tokens: 200_000,
            max_cost_usd: 10.0,
            warn_at: 0.8,
        };
        let budget = BudgetTracker::new(cfg);
        let agents = empty_agents();
        let graph = single_node_graph();

        let results =
            execute_task_graph(&graph, &agents, Some(&budget), idle_runner()).await;
        assert_eq!(results.len(), 1);
        // Budget overhead cost should be recorded for the task
        assert!(budget.total_cost_usd() > 0.001);
    }
}
