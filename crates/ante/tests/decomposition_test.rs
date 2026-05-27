//! Integration test for sub-agent loading and decomposition.
//! Tests T031: Verify agent loading, task decomposition, and result synthesis.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

/// Helper: write a sub-agent .md file with YAML frontmatter.
fn write_agent(dir: &PathBuf, name: &str, description: &str, role: &str, prompt: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = dir.join(format!("{name}-{count}.md"));
    let content = format!(
        "---\nname: {name}\ndescription: {description}\nrole: {role}\n---\n\n{prompt}\n"
    );
    std::fs::write(&path, content).expect("write agent file");
    path
}

#[tokio::test]
async fn test_agent_loading_and_retrieval() {
    let tmp = TempDir::new().unwrap();
    let agents_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();

    write_agent(&agents_dir, "reader", "Reads files from disk", "reader", "You read files.");
    write_agent(&agents_dir, "writer", "Writes files to disk", "writer", "You write files.");

    let registry = agent_sdk::agents::AgentRegistry::load(&agents_dir).unwrap();
    let all = registry.all();
    assert_eq!(all.len(), 2, "should load 2 agents");

    let names: Vec<&str> = all.iter().map(|a| a.name.as_str()).collect();
    assert!(names.contains(&"reader"));
    assert!(names.contains(&"writer"));
}

#[tokio::test]
async fn test_task_decomposition() {
    let tmp = TempDir::new().unwrap();
    let agents_dir = tmp.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    write_agent(&agents_dir, "reader", "Reads files", "reader", "You read files.");
    write_agent(&agents_dir, "analyzer", "Analyzes content", "analyzer", "You analyze output.");

    let registry = agent_sdk::agents::AgentRegistry::load(&agents_dir).unwrap();
    let graph = agent_sdk::agents::decompose_request(
        "read the file, then analyze the output",
        &registry,
    );

    assert!(graph.tasks.len() >= 2, "should decompose into at least 2 tasks: {:?}", graph.tasks);
    for t in &graph.tasks {
        assert!(!t.description.is_empty(), "no empty task descriptions");
    }
}

#[tokio::test]
async fn test_dispatch_and_synthesize() {
    use agent_sdk::agents::dispatcher::{TaskResult, synthesize_results};

    let result_a = TaskResult {
        task_id: "1".into(),
        description: "read the file".into(),
        agent: Some("reader".into()),
        output: "File contents: hello".into(),
        error: None,
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
    };
    let result_b = TaskResult {
        task_id: "2".into(),
        description: "analyze output".into(),
        agent: Some("analyzer".into()),
        output: "Analysis: contains greeting".into(),
        error: None,
        input_tokens: 0,
        output_tokens: 0,
        cost_usd: 0.0,
    };
    let results = vec![result_a, result_b];

    let synthesized = synthesize_results(&results);
    assert!(synthesized.contains("File contents"), "should include first result");
    assert!(synthesized.contains("Analysis"), "should include second result");
}

#[tokio::test]
async fn test_task_graph_creation() {
    use agent_sdk::agents::loader::TaskNode;

    let graph = agent_sdk::agents::TaskGraph {
        tasks: vec![
            TaskNode {
                id: "a".into(),
                description: "step a".into(),
                assigned_agent: Some("agent-a".into()),
                dependencies: vec![],
            },
            TaskNode {
                id: "b".into(),
                description: "step b".into(),
                assigned_agent: Some("agent-b".into()),
                dependencies: vec!["a".into()],
            },
        ],
    };

    assert_eq!(graph.tasks.len(), 2);
    assert_eq!(graph.tasks[0].id, "a");
    assert_eq!(graph.tasks[1].dependencies, vec!["a"]);
}
