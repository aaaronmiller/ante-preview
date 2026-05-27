//! Model router — selects the appropriate model based on task complexity.
//!
//! Phase 1: Rule-based classifier using keyword scoring + token budget.
//! Phase 2: ML-based routing via configurable scoring.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A configured model pool entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPoolEntry {
    /// Model identifier (e.g. "claude-sonnet-4", "claude-haiku-3.5").
    pub model: String,
    /// Capability score 1-10 (10 = most capable).
    pub capability: u8,
    /// Cost per 1K input tokens (USD cents).
    pub cost_per_1k_input: f64,
    /// Cost per 1K output tokens (USD cents).
    pub cost_per_1k_output: f64,
    /// Maximum context tokens.
    pub max_context: u32,
}

/// A routing decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDecision {
    pub selected_model: String,
    pub reason: String,
    pub estimated_cost_cents: f64,
    pub estimated_tokens: u32,
}

/// Errors from the model router.
#[derive(Debug, Error)]
pub enum RouterError {
    #[error("No models configured")]
    NoModels,

    #[error("No model meets task requirements")]
    NoSuitableModel,

    #[error("Ambiguous routing: multiple equal matches")]
    Ambiguous,
}

/// Rule-based model router.
pub struct ModelRouter {
    models: Vec<ModelPoolEntry>,
}

impl ModelRouter {
    /// Create a new router from a model pool.
    pub fn new(models: Vec<ModelPoolEntry>) -> Self {
        let mut sorted = models;
        sorted.sort_by(|a, b| b.capability.cmp(&a.capability));
        ModelRouter { models: sorted }
    }

    /// Classify a task and select the cheapest adequate model.
    ///
    /// Strategy:
    /// 1. Estimate required tokens from task length + tool complexity
    /// 2. Classify task complexity (simple / moderate / complex)
    /// 3. Find cheapest model that meets capability threshold
    pub fn select(&self, task: &str, tool_count: usize) -> Result<RouterDecision, RouterError> {
        if self.models.is_empty() {
            return Err(RouterError::NoModels);
        }

        let estimated_tokens = estimate_tokens(task, tool_count);
        let complexity = classify_complexity(task);

        let required_capability = match complexity {
            TaskComplexity::Simple => 3,
            TaskComplexity::Moderate => 5,
            TaskComplexity::Complex => 8,
        };

        // Find cheapest model meeting capability + context requirements
        let mut candidates: Vec<&ModelPoolEntry> = self
            .models
            .iter()
            .filter(|m| m.capability >= required_capability && m.max_context >= estimated_tokens)
            .collect();

        if candidates.is_empty() {
            // Fall back to most capable model
            candidates = self.models.iter().collect();
        }

        // Sort by cost (cheapest first)
        candidates.sort_by(|a, b| {
            a.cost_per_1k_input
                .partial_cmp(&b.cost_per_1k_input)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let selected = candidates[0];
        let input_cost = selected.cost_per_1k_input * estimated_tokens as f64 / 1000.0;
        let output_cost = selected.cost_per_1k_output * (estimated_tokens as f64 / 4.0) / 1000.0;

        Ok(RouterDecision {
            selected_model: selected.model.clone(),
            reason: format!("{} task (cap={}, cost=${:.4})", 
                complexity.as_str(), selected.capability, input_cost + output_cost),
            estimated_cost_cents: input_cost + output_cost,
            estimated_tokens,
        })
    }

    /// Get all configured models.
    pub fn models(&self) -> &[ModelPoolEntry] {
        &self.models
    }

    /// Number of configured models.
    pub fn count(&self) -> usize {
        self.models.len()
    }
}

/// Task complexity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    Simple,
    Moderate,
    Complex,
}

impl TaskComplexity {
    fn as_str(&self) -> &'static str {
        match self {
            TaskComplexity::Simple => "simple",
            TaskComplexity::Moderate => "moderate",
            TaskComplexity::Complex => "complex",
        }
    }
}

/// Estimate token count from task description + tool count.
fn estimate_tokens(task: &str, tool_count: usize) -> u32 {
    let base = task.len() as u32 / 4; // ~4 chars per token
    let tool_overhead = (tool_count as u32) * 500; // system prompt per tool
    let min = 2000u32;
    base + tool_overhead + min
}

/// Classify task complexity based on keywords.
fn classify_complexity(task: &str) -> TaskComplexity {
    let lower = task.to_lowercase();

    // Complex indicators: multi-step, analysis, architecture, security, data
    let complex_keywords = [
        "architect", "design", "analyze", "security", "deploy",
        "refactor", "optimize", "migrate", "pipeline", "infrastructure",
        "multi-step", "orchestrat", "distributed", "asynchronous",
    ];
    let has_complex = complex_keywords.iter().any(|k| lower.contains(k));

    // Simple indicators: list, read, search, copy, rename, format
    let simple_keywords = [
        "list", "read", "show", "search", "find", "grep",
        "copy", "move", "rename", "format", "lint",
        "count", "sort", "echo",
    ];
    let has_simple = simple_keywords.iter().any(|k| lower.contains(k));

    // Moderately complex keywords
    let moderate_keywords = [
        "build", "write", "create", "edit", "update", "test",
        "configure", "install", "compile", "run",
    ];
    let has_moderate = moderate_keywords.iter().any(|k| lower.contains(k));

    if has_complex {
        TaskComplexity::Complex
    } else if has_moderate && !has_simple {
        TaskComplexity::Moderate
    } else if has_simple && !has_moderate && !has_complex {
        TaskComplexity::Simple
    } else {
        // Default to moderate for ambiguous tasks
        TaskComplexity::Moderate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Vec<ModelPoolEntry> {
        vec![
            ModelPoolEntry {
                model: "claude-haiku-3.5".into(),
                capability: 4,
                cost_per_1k_input: 0.25,
                cost_per_1k_output: 1.25,
                max_context: 200_000,
            },
            ModelPoolEntry {
                model: "claude-sonnet-4".into(),
                capability: 9,
                cost_per_1k_input: 3.00,
                cost_per_1k_output: 15.00,
                max_context: 200_000,
            },
            ModelPoolEntry {
                model: "claude-opus-4".into(),
                capability: 10,
                cost_per_1k_input: 15.00,
                cost_per_1k_output: 75.00,
                max_context: 200_000,
            },
        ]
    }

    #[test]
    fn simple_task_selects_cheapest_adequate() {
        let router = ModelRouter::new(test_pool());
        let decision = router.select("list files in directory", 1).unwrap();
        // Haiku (cap=4) should be adequate for simple tasks
        assert_eq!(decision.selected_model, "claude-haiku-3.5");
    }

    #[test]
    fn complex_task_selects_powerful_model() {
        let router = ModelRouter::new(test_pool());
        let decision = router.select("design system architecture for microservices", 5).unwrap();
        // Should pick Sonnet or Opus (cap >= 8)
        assert!(decision.selected_model.contains("sonnet") || decision.selected_model.contains("opus"));
    }

    #[test]
    fn empty_pool_errors() {
        let router = ModelRouter::new(vec![]);
        assert!(router.select("anything", 0).is_err());
    }

    #[test]
    fn classification_identifies_simple() {
        assert_eq!(classify_complexity("list files"), TaskComplexity::Simple);
        assert_eq!(classify_complexity("search for pattern"), TaskComplexity::Simple);
    }

    #[test]
    fn classification_identifies_complex() {
        assert_eq!(
            classify_complexity("design distributed architecture"),
            TaskComplexity::Complex
        );
        assert_eq!(
            classify_complexity("secure the deployment pipeline"),
            TaskComplexity::Complex
        );
    }

    #[test]
    fn classification_identifies_moderate() {
        assert_eq!(classify_complexity("build a web server"), TaskComplexity::Moderate);
        assert_eq!(classify_complexity("write unit tests"), TaskComplexity::Moderate);
    }

    #[test]
    fn token_estimate_increases_with_tools() {
        let t1 = estimate_tokens("hello", 0);
        let t2 = estimate_tokens("hello", 10);
        assert!(t2 > t1);
    }

    #[test]
    fn router_sorts_by_capability() {
        let router = ModelRouter::new(test_pool());
        assert_eq!(router.count(), 3);
        assert_eq!(router.models()[0].capability, 10); // most capable first
    }
}
