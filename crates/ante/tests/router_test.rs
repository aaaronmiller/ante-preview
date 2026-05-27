//! Integration test for model router.
//! Tests T048: Verify router selects correct model based on task complexity,
//! cost ordering, and fallback behavior.

use agent_sdk::router::{ModelRouter, ModelPoolEntry};

fn test_pool() -> Vec<ModelPoolEntry> {
    vec![
        ModelPoolEntry {
            model: "claude-sonnet-4".into(),
            capability: 8,
            cost_per_1k_input: 3.0,
            cost_per_1k_output: 15.0,
            max_context: 200_000,
        },
        ModelPoolEntry {
            model: "claude-haiku-4".into(),
            capability: 4,
            cost_per_1k_input: 1.0,
            cost_per_1k_output: 5.0,
            max_context: 100_000,
        },
    ]
}

#[tokio::test]
async fn test_router_selects_cheapest_for_simple_task() {
    let pool = test_pool();
    let router = ModelRouter::new(pool);

    let decision = router.select("list the files in the current directory", 1).unwrap();
    // Simple task should pick the cheaper model (haiku)
    assert_eq!(decision.selected_model, "claude-haiku-4", "simple: pick cheapest capable");
}

#[tokio::test]
async fn test_router_selects_capable_for_complex_task() {
    let pool = test_pool();
    let router = ModelRouter::new(pool);

    let decision = router.select("design a distributed system architecture with microservices, event sourcing, and CQRS patterns", 5).unwrap();
    // Complex task should pick the more capable model (sonnet)
    assert_eq!(decision.selected_model, "claude-sonnet-4", "complex: pick most capable");
}

#[tokio::test]
async fn test_router_handles_empty_pool() {
    let router = ModelRouter::new(vec![]);
    let result = router.select("do something", 0);
    assert!(result.is_err(), "empty pool should produce error");
}

#[tokio::test]
async fn test_router_capability_ordering() {
    let pool = vec![
        ModelPoolEntry {
            model: "model-a".into(),
            capability: 5,
            cost_per_1k_input: 2.0,
            cost_per_1k_output: 10.0,
            max_context: 100_000,
        },
        ModelPoolEntry {
            model: "model-b".into(),
            capability: 3,
            cost_per_1k_input: 1.0,
            cost_per_1k_output: 5.0,
            max_context: 100_000,
        },
        ModelPoolEntry {
            model: "model-c".into(),
            capability: 8,
            cost_per_1k_input: 4.0,
            cost_per_1k_output: 20.0,
            max_context: 200_000,
        },
    ];
    let router = ModelRouter::new(pool);

    // Complex task should pick highest capability
    let decision = router.select("design a complex system with many components", 3).unwrap();
    assert_eq!(decision.selected_model, "model-c", "complex: highest capability");
}
