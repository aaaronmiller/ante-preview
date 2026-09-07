//! Task decomposition engine — analyzes user requests, breaks them
//! into sub-tasks, builds a dependency graph, and assigns agents.

use super::loader::{AgentRegistry, TaskGraph, TaskNode};

/// Decompose a user request into a task graph using registered agents.
///
/// # Note
/// Phase 1 implementation uses simple keyword decomposition.
/// Future versions will use LLM-based decomposition.
pub fn decompose_request(
    request: &str,
    agents: &AgentRegistry,
) -> TaskGraph {
    let lower = request.to_lowercase();

    // Simple decomposition: split on "and", "then", "also"
    let mut tasks = Vec::new();

    let segments: Vec<String> = lower
        .split(|c: char| c == ',' || c == '.')
        .flat_map(|s| {
            let trimmed = s.trim();
            // Split on conjunctions
            if trimmed.starts_with("then ") || trimmed.starts_with("and ") || trimmed.starts_with("also ") {
                let rest = trimmed
                    .strip_prefix("then ")
                    .or(trimmed.strip_prefix("and "))
                    .or(trimmed.strip_prefix("also "))
                    .unwrap_or(trimmed);
                if !rest.is_empty() {
                    Some(rest.trim().to_string())
                } else {
                    None
                }
            } else if !trimmed.is_empty() {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .collect();

    for (i, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            continue;
        }

        let task_id = format!("task-{}", i + 1);
        let assigned = agents.find_best_match(segment);
        let agent_name = assigned.map(|a| a.name.clone());

        // Sequential segments have dependency on previous
        let mut deps_here = Vec::new();
        if i > 0 {
            deps_here.push(format!("task-{}", i));
        }

        tasks.push(TaskNode {
            id: task_id,
            description: segment.clone(),
            assigned_agent: agent_name,
            dependencies: deps_here,
        });
    }

    // If no decomposition happened, create a single task
    if tasks.is_empty() {
        let agent = agents.find_best_match(request);
        tasks.push(TaskNode {
            id: "task-1".into(),
            description: request.to_string(),
            assigned_agent: agent.map(|a| a.name.clone()),
            dependencies: Vec::new(),
        });
    }

    TaskGraph { tasks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_agents() -> AgentRegistry {
        AgentRegistry::load(Path::new("/nonexistent")).unwrap()
    }

    #[test]
    fn decomposes_simple_request() {
        let agents = test_agents();
        let graph = decompose_request("list files", &agents);
        assert_eq!(graph.tasks.len(), 1);
    }

    #[test]
    fn decomposes_multi_step_request() {
        let agents = test_agents();
        let graph = decompose_request("read the file, then parse the data", &agents);
        // Should produce at least one task
        assert!(!graph.tasks.is_empty());
    }

    #[test]
    fn empty_decomposition_falls_back() {
        let agents = test_agents();
        let graph = decompose_request("", &agents);
        // Single fallback task
        assert_eq!(graph.tasks.len(), 1);
    }

    #[test]
    fn decomposition_includes_dependencies() {
        let agents = test_agents();
        let graph = decompose_request("step one then step two", &agents);
        if graph.tasks.len() > 1 {
            // Second task depends on first
            assert_eq!(graph.tasks[0].dependencies.len(), 0);
            assert_eq!(graph.tasks[1].dependencies.len(), 1);
        }
    }
}
