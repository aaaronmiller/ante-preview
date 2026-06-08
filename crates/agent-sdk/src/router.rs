//! Model router — selects the appropriate model based on task complexity.
//!
//! Phase 1: Rule-based classifier using keyword scoring + token budget.
//! Phase 2: ML-based routing via configurable scoring.
//!
//! Includes a feedback loop that adjusts per-model capability scores
//! based on observed success/failure, and a fallback chain that tries
//! progressively cheaper/weaker models when the primary selection fails.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const FEEDBACK_WINDOW: usize = 100; // How many recent entries to consider
const FAILURE_PENALTY_THRESHOLD: f64 = 0.25; // Fraction of failures that triggers a penalty
const CAPABILITY_PENALTY: u8 = 2; // Points subtracted from capability on sustained failures
const RECOVERY_WINDOW: usize = 20; // Consecutive successes needed to lift a penalty
const FALLBACK_MAX_RETRIES: usize = 3; // Maximum fallback attempts

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

/// Feedback about a routing outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingFeedback {
    /// The model that was selected.
    pub model: String,
    /// Whether the turn/tool-call succeeded.
    pub success: bool,
    /// Latency in milliseconds.
    pub latency_ms: u64,
    /// Input tokens consumed.
    pub input_tokens: u64,
    /// Output tokens consumed.
    pub output_tokens: u64,
    /// Optional error category for failures (e.g., "timeout", "rate_limit", "invalid_response").
    pub error_category: Option<String>,
}

impl RoutingFeedback {
    /// Create a success feedback entry.
    pub fn success(model: String, latency_ms: u64, input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            model,
            success: true,
            latency_ms,
            input_tokens,
            output_tokens,
            error_category: None,
        }
    }

    /// Create a failure feedback entry.
    pub fn failure(model: String, latency_ms: u64, error_category: Option<String>) -> Self {
        Self {
            model,
            success: false,
            latency_ms,
            input_tokens: 0,
            output_tokens: 0,
            error_category,
        }
    }
}

/// Internal tracked feedback entry with timestamp.
#[derive(Debug, Clone)]
struct FeedbackEntry {
    feedback: RoutingFeedback,
    _timestamp: std::time::Instant,
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

    #[error("All fallback models exhausted")]
    AllFallbacksExhausted,

    #[error("Fallback failed: {0}")]
    FallbackFailed(String),
}

/// Rule-based model router with feedback learning and fallback support.
pub struct ModelRouter {
    models: Vec<ModelPoolEntry>,
    feedback_history: VecDeque<FeedbackEntry>,
    /// Track consecutive successes per model (for recovery from penalty).
    consecutive_successes: std::collections::HashMap<String, usize>,
    /// Track whether a model is currently penalized.
    penalized_models: std::collections::HashSet<String>,
}

impl ModelRouter {
    /// Create a new router from a model pool.
    pub fn new(models: Vec<ModelPoolEntry>) -> Self {
        let mut sorted = models;
        sorted.sort_by(|a, b| b.capability.cmp(&a.capability));
        ModelRouter {
            models: sorted,
            feedback_history: VecDeque::with_capacity(FEEDBACK_WINDOW),
            consecutive_successes: std::collections::HashMap::new(),
            penalized_models: std::collections::HashSet::new(),
        }
    }

    // ─── Core Selection ──────────────────────────────────────────────────

    /// Classify a task and select the cheapest adequate model.
    ///
    /// Strategy:
    /// 1. Estimate required tokens from task length + tool complexity
    /// 2. Classify task complexity (simple / moderate / complex)
    /// 3. Find cheapest model that meets effective capability threshold
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

        // Use effective capability (adjusted by feedback) for filtering
        let mut candidates: Vec<&ModelPoolEntry> = self
            .models
            .iter()
            .filter(|m| {
                let eff_cap = self.effective_capability(&m.model);
                eff_cap >= required_capability && m.max_context >= estimated_tokens
            })
            .collect();

        if candidates.is_empty() {
            // Fall back to most capable model regardless of penalty
            candidates = self.models.iter().collect();
        }

        // Sort by cost (cheapest first), but also prefer non-penalized models
        candidates.sort_by(|a, b| {
            let a_penalized = self.penalized_models.contains(&a.model);
            let b_penalized = self.penalized_models.contains(&b.model);
            // Non-penalized models are preferred
            match a_penalized.cmp(&b_penalized) {
                std::cmp::Ordering::Equal => a
                    .cost_per_1k_input
                    .partial_cmp(&b.cost_per_1k_input)
                    .unwrap_or(std::cmp::Ordering::Equal),
                other => other,
            }
        });

        let selected = candidates[0];
        let input_cost = selected.cost_per_1k_input * estimated_tokens as f64 / 1000.0;
        let output_cost = selected.cost_per_1k_output * (estimated_tokens as f64 / 4.0) / 1000.0;

        let penalty_flag = if self.penalized_models.contains(&selected.model) {
            " (penalized)"
        } else {
            ""
        };

        Ok(RouterDecision {
            selected_model: selected.model.clone(),
            reason: format!(
                "{} task (eff_cap={}, cost=${:.4}){}",
                complexity.as_str(),
                self.effective_capability(&selected.model),
                input_cost + output_cost,
                penalty_flag
            ),
            estimated_cost_cents: input_cost + output_cost,
            estimated_tokens,
        })
    }

    /// Select a model with fallback. Returns the first successful selection;
    /// if `feedback_collector` is provided, records failures for each
    /// attempted model and tries the next candidate.
    ///
    /// The iterator `feedback_collector` is called for each attempted model
    /// and should return `Ok(())` on success or `Err(feedback)` on failure.
    pub async fn select_with_fallback<F, Fut>(
        &self,
        task: &str,
        tool_count: usize,
        mut feedback_collector: F,
    ) -> Result<RouterDecision, RouterError>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<(), RoutingFeedback>>,
    {
        if self.models.is_empty() {
            return Err(RouterError::NoModels);
        }

        // Build an ordered candidate list (cheapest adequate first, but
        // in fallback mode we try them in ascending cost order).
        let estimated_tokens = estimate_tokens(task, tool_count);
        let complexity = classify_complexity(task);
        let required_capability = match complexity {
            TaskComplexity::Simple => 3,
            TaskComplexity::Moderate => 5,
            TaskComplexity::Complex => 8,
        };

        let mut candidates: Vec<&ModelPoolEntry> = self
            .models
            .iter()
            .filter(|m| {
                self.effective_capability(&m.model) >= required_capability
                    || m.max_context >= estimated_tokens
            })
            .collect();

        if candidates.is_empty() {
            candidates = self.models.iter().collect();
        }

        // Sort by cost ascending (cheapest first for fallback)
        candidates.sort_by(|a, b| {
            a.cost_per_1k_input
                .partial_cmp(&b.cost_per_1k_input)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut last_error: Option<String> = None;
        let mut attempts = 0;

        for candidate in &candidates {
            if attempts >= FALLBACK_MAX_RETRIES {
                break;
            }
            attempts += 1;

            match feedback_collector(candidate.model.clone()).await {
                Ok(()) => {
                    let input_cost = candidate.cost_per_1k_input * estimated_tokens as f64 / 1000.0;
                    let output_cost =
                        candidate.cost_per_1k_output * (estimated_tokens as f64 / 4.0) / 1000.0;
                    return Ok(RouterDecision {
                        selected_model: candidate.model.clone(),
                        reason: format!(
                            "fallback-selected: {} (attempt {})",
                            candidate.model, attempts
                        ),
                        estimated_cost_cents: input_cost + output_cost,
                        estimated_tokens,
                    });
                }
                Err(feedback) => {
                    last_error = Some(
                        feedback
                            .error_category
                            .unwrap_or_else(|| "unknown".to_string()),
                    );
                }
            }
        }

        if let Some(error) = last_error {
            Err(RouterError::FallbackFailed(error))
        } else {
            Err(RouterError::AllFallbacksExhausted)
        }
    }

    // ─── Feedback Loop ───────────────────────────────────────────────────

    /// Record feedback about a routing outcome.
    ///
    /// Adjusts internal per-model capability scores based on sustained
    /// success or failure patterns.
    pub fn record_feedback(&mut self, feedback: RoutingFeedback) {
        let model = feedback.model.clone();

        // Trim history if at capacity
        if self.feedback_history.len() >= FEEDBACK_WINDOW {
            self.feedback_history.pop_front();
        }

        self.feedback_history.push_back(FeedbackEntry {
            feedback,
            _timestamp: std::time::Instant::now(),
        });

        // Update consecutive successes counter
        let entry = self.feedback_history.back().unwrap();
        let consecutive = self.consecutive_successes.entry(model.clone()).or_insert(0);

        if entry.feedback.success {
            *consecutive += 1;

            // If we've recovered enough consecutive successes, lift the penalty
            if *consecutive >= RECOVERY_WINDOW {
                self.penalized_models.remove(&model);
            }
        } else {
            *consecutive = 0;

            // Check if failure rate warrants a penalty
            let failure_rate = self.model_failure_rate(&model);
            if failure_rate >= FAILURE_PENALTY_THRESHOLD {
                self.penalized_models.insert(model);
            }
        }
    }

    /// Compute the effective capability of a model, accounting for
    /// feedback-based penalties.
    fn effective_capability(&self, model: &str) -> u8 {
        let base = self
            .models
            .iter()
            .find(|m| m.model == model)
            .map(|m| m.capability)
            .unwrap_or(1);

        if self.penalized_models.contains(model) {
            base.saturating_sub(CAPABILITY_PENALTY).max(1)
        } else {
            base
        }
    }

    /// Compute the failure rate for a model within the feedback window.
    fn model_failure_rate(&self, model: &str) -> f64 {
        let (total, failures) = self
            .feedback_history
            .iter()
            .filter(|e| e.feedback.model == model)
            .fold((0usize, 0usize), |(t, f), e| {
                (t + 1, f + if e.feedback.success { 0 } else { 1 })
            });

        if total == 0 {
            0.0
        } else {
            failures as f64 / total as f64
        }
    }

    /// Get feedback statistics for a model.
    pub fn model_stats(&self, model: &str) -> ModelStats {
        let (total, failures) = self
            .feedback_history
            .iter()
            .filter(|e| e.feedback.model == model)
            .fold((0usize, 0usize), |(t, f), e| {
                (t + 1, f + if e.feedback.success { 0 } else { 1 })
            });

        let base_cap = self
            .models
            .iter()
            .find(|m| m.model == model)
            .map(|m| m.capability)
            .unwrap_or(0);

        ModelStats {
            model: model.to_string(),
            total_attempts: total,
            total_failures: failures,
            failure_rate: if total > 0 {
                failures as f64 / total as f64
            } else {
                0.0
            },
            base_capability: base_cap,
            effective_capability: self.effective_capability(model),
            penalized: self.penalized_models.contains(model),
        }
    }

    /// Get stats for all models.
    pub fn all_stats(&self) -> Vec<ModelStats> {
        self.models
            .iter()
            .map(|m| self.model_stats(&m.model))
            .collect()
    }

    // ─── Accessors ───────────────────────────────────────────────────────

    /// Get all configured models.
    pub fn models(&self) -> &[ModelPoolEntry] {
        &self.models
    }

    /// Number of configured models.
    pub fn count(&self) -> usize {
        self.models.len()
    }
}

/// Statistics for a single model in the pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStats {
    pub model: String,
    pub total_attempts: usize,
    pub total_failures: usize,
    pub failure_rate: f64,
    pub base_capability: u8,
    pub effective_capability: u8,
    pub penalized: bool,
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
        "architect",
        "design",
        "analyze",
        "security",
        "deploy",
        "refactor",
        "optimize",
        "migrate",
        "pipeline",
        "infrastructure",
        "multi-step",
        "orchestrat",
        "distributed",
        "asynchronous",
    ];
    let has_complex = complex_keywords.iter().any(|k| lower.contains(k));

    // Simple indicators: list, read, search, copy, rename, format
    let simple_keywords = [
        "list", "read", "show", "search", "find", "grep", "copy", "move", "rename", "format",
        "lint", "count", "sort", "echo",
    ];
    let has_simple = simple_keywords.iter().any(|k| lower.contains(k));

    // Moderately complex keywords
    let moderate_keywords = [
        "build",
        "write",
        "create",
        "edit",
        "update",
        "test",
        "configure",
        "install",
        "compile",
        "run",
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
        assert_eq!(decision.selected_model, "claude-haiku-3.5");
    }

    #[test]
    fn complex_task_selects_powerful_model() {
        let router = ModelRouter::new(test_pool());
        let decision = router
            .select("design system architecture for microservices", 5)
            .unwrap();
        assert!(
            decision.selected_model.contains("sonnet") || decision.selected_model.contains("opus")
        );
    }

    #[test]
    fn empty_pool_errors() {
        let router = ModelRouter::new(vec![]);
        assert!(router.select("anything", 0).is_err());
    }

    #[test]
    fn classification_identifies_simple() {
        assert_eq!(classify_complexity("list files"), TaskComplexity::Simple);
        assert_eq!(
            classify_complexity("search for pattern"),
            TaskComplexity::Simple
        );
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
        assert_eq!(
            classify_complexity("build a web server"),
            TaskComplexity::Moderate
        );
        assert_eq!(
            classify_complexity("write unit tests"),
            TaskComplexity::Moderate
        );
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
        assert_eq!(router.models()[0].capability, 10);
    }

    // ─── Feedback Loop Tests ──────────────────────────────────────────────

    #[test]
    fn feedback_success_does_not_penalize() {
        let mut router = ModelRouter::new(test_pool());
        // Record many successes
        for _ in 0..10 {
            router.record_feedback(RoutingFeedback::success(
                "claude-haiku-3.5".into(),
                100,
                100,
                50,
            ));
        }
        let stats = router.model_stats("claude-haiku-3.5");
        assert!(!stats.penalized);
        assert_eq!(stats.effective_capability, stats.base_capability);
    }

    #[test]
    fn feedback_failures_trigger_penalty() {
        let mut router = ModelRouter::new(test_pool());
        // Record enough failures to cross threshold (25% of window=100 → 25 failures)
        for _ in 0..30 {
            router.record_feedback(RoutingFeedback::failure(
                "claude-haiku-3.5".into(),
                500,
                Some("timeout".into()),
            ));
        }
        let stats = router.model_stats("claude-haiku-3.5");
        assert!(stats.penalized);
        assert!(stats.effective_capability < stats.base_capability);
    }

    #[test]
    fn feedback_failures_affect_selection() {
        let mut router = ModelRouter::new(test_pool());
        // Penalize Haiku by recording many failures
        for _ in 0..30 {
            router.record_feedback(RoutingFeedback::failure(
                "claude-haiku-3.5".into(),
                500,
                Some("timeout".into()),
            ));
        }

        // Haiku's effective capability should be reduced
        assert_eq!(router.effective_capability("claude-haiku-3.5"), 2); // 4 - 2 = 2

        // A simple task should still select something (not error)
        let decision = router.select("list files", 0).unwrap();
        // If Haiku is penalized below threshold (cap 2 < required 3), it shouldn't be selected
        assert_ne!(decision.selected_model, "claude-haiku-3.5");
    }

    #[test]
    fn recovery_from_penalty() {
        let mut router = ModelRouter::new(test_pool());

        // First, trigger penalty with failures
        for _ in 0..30 {
            router.record_feedback(RoutingFeedback::failure(
                "claude-haiku-3.5".into(),
                500,
                Some("timeout".into()),
            ));
        }
        assert!(router.model_stats("claude-haiku-3.5").penalized);

        // Then recover with consecutive successes
        for _ in 0..RECOVERY_WINDOW {
            router.record_feedback(RoutingFeedback::success(
                "claude-haiku-3.5".into(),
                50,
                100,
                50,
            ));
        }

        let stats = router.model_stats("claude-haiku-3.5");
        assert!(!stats.penalized);
        assert_eq!(stats.effective_capability, stats.base_capability);
    }

    #[test]
    fn model_stats_accumulates_correctly() {
        let mut router = ModelRouter::new(test_pool());

        router.record_feedback(RoutingFeedback::success("sonnet".into(), 100, 200, 100));
        router.record_feedback(RoutingFeedback::failure(
            "sonnet".into(),
            500,
            Some("rate_limit".into()),
        ));
        router.record_feedback(RoutingFeedback::success("sonnet".into(), 150, 300, 150));

        let stats = router.model_stats("sonnet");
        assert_eq!(stats.total_attempts, 3);
        assert_eq!(stats.total_failures, 1);
        assert!((stats.failure_rate - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn all_stats_returns_all_models() {
        let router = ModelRouter::new(test_pool());
        let stats = router.all_stats();
        assert_eq!(stats.len(), 3);
    }

    // ─── Fallback Tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn fallback_succeeds_on_first_try() {
        let router = ModelRouter::new(test_pool());

        let decision = router
            .select_with_fallback("list files", 0, |model| async move {
                assert_eq!(model, "claude-haiku-3.5");
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(decision.selected_model, "claude-haiku-3.5");
    }

    #[tokio::test]
    async fn fallback_retries_on_failure() {
        let router = ModelRouter::new(test_pool());
        let attempts = std::sync::atomic::AtomicUsize::new(0);

        router
            .select_with_fallback("list files", 0, |_model| {
                let n = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    if n < 1 {
                        Err(RoutingFeedback::failure(
                            "model".into(),
                            500,
                            Some("timeout".into()),
                        ))
                    } else {
                        Ok(())
                    }
                }
            })
            .await
            .unwrap();

        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn fallback_exhausts_retries() {
        let router = ModelRouter::new(test_pool());

        let result = router
            .select_with_fallback("list files", 0, |_model| async move {
                Err(RoutingFeedback::failure(
                    "model".into(),
                    500,
                    Some("crash".into()),
                ))
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::AllFallbacksExhausted)));
    }
}
