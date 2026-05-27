//! Context budget tracker — monitors token and cost usage against
//! configured limits, emitting warnings when approaching thresholds.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use thiserror::Error;

/// Configuration for context budget tracking.
#[derive(Debug, Clone)]
pub struct BudgetConfig {
    /// Maximum tokens in context before compaction is required.
    pub max_context_tokens: u64,
    /// Maximum cost in USD before a warning is raised.
    pub max_cost_usd: f64,
    /// Percentage of limit at which to emit a warning (0.0 - 1.0).
    pub warn_at: f64,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 200_000,
            max_cost_usd: 1.0,
            warn_at: 0.8,
        }
    }
}

/// Tracks token and cost usage during a session.
///
/// Uses `Arc<AtomicU64>` for cheap concurrent reads/writes without locks.
#[derive(Debug, Clone)]
pub struct BudgetTracker {
    total_input_tokens: Arc<AtomicU64>,
    total_output_tokens: Arc<AtomicU64>,
    total_cost_millicents: Arc<AtomicU64>, // stored as 1/100,000 USD to avoid float atomics
    config: BudgetConfig,
}

#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("Context token limit exceeded: {used} >= {limit}")]
    TokenOverLimit { used: u64, limit: u64 },

    #[error("Cost limit exceeded: ${0:.5}")]
    CostOverLimit(f64),
}

impl BudgetTracker {
    /// Create a new tracker with the given configuration.
    pub fn new(config: BudgetConfig) -> Self {
        Self {
            total_input_tokens: Arc::new(AtomicU64::new(0)),
            total_output_tokens: Arc::new(AtomicU64::new(0)),
            total_cost_millicents: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// Add input tokens (incremental).
    pub fn add_input_tokens(&self, count: u64) {
        self.total_input_tokens.fetch_add(count, Ordering::Relaxed);
    }

    /// Add output tokens (incremental).
    pub fn add_output_tokens(&self, count: u64) {
        self.total_output_tokens.fetch_add(count, Ordering::Relaxed);
    }

    /// Add cost in USD (converted to millicents internally).
    pub fn add_cost(&self, usd: f64) {
        let millicents = (usd * 100_000.0).round() as u64;
        self.total_cost_millicents.fetch_add(millicents, Ordering::Relaxed);
    }

    /// Total tokens used (input + output).
    pub fn total_tokens(&self) -> u64 {
        self.total_input_tokens.load(Ordering::Relaxed)
            + self.total_output_tokens.load(Ordering::Relaxed)
    }

    /// Total cost in USD.
    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_millicents.load(Ordering::Relaxed) as f64 / 100_000.0
    }

    /// Returns a warning message if approaching limits, otherwise None.
    pub fn warn_message(&self) -> Option<String> {
        let tokens = self.total_tokens();
        let cost = self.total_cost_usd();
        let warnings: Vec<String> = {
            let mut v = Vec::new();
            if self.config.max_context_tokens > 0 {
                let ratio = tokens as f64 / self.config.max_context_tokens as f64;
                if ratio >= self.config.warn_at {
                    v.push(format!(
                        "tokens at {:.0}% ({}/{})",
                        ratio * 100.0,
                        tokens,
                        self.config.max_context_tokens
                    ));
                }
            }
            if self.config.max_cost_usd > 0.0 {
                let ratio = cost / self.config.max_cost_usd;
                if ratio >= self.config.warn_at {
                    v.push(format!("cost at ${cost:.4} (limit: ${limit:.4})",
                        limit = self.config.max_cost_usd));
                }
            }
            v
        };

        if warnings.is_empty() {
            None
        } else {
            Some(warnings.join("; "))
        }
    }

    /// Returns true if any limit is exceeded.
    pub fn is_over_limit(&self) -> bool {
        if self.config.max_context_tokens > 0 && self.total_tokens() >= self.config.max_context_tokens
        {
            return true;
        }
        if self.config.max_cost_usd > 0.0 && self.total_cost_usd() >= self.config.max_cost_usd {
            return true;
        }
        false
    }

    /// Check limits and return an error if exceeded.
    pub fn check_limits(&self) -> Result<(), BudgetError> {
        let tokens = self.total_tokens();
        let cost = self.total_cost_usd();

        if self.config.max_context_tokens > 0 && tokens >= self.config.max_context_tokens {
            return Err(BudgetError::TokenOverLimit {
                used: tokens,
                limit: self.config.max_context_tokens,
            });
        }
        if self.config.max_cost_usd > 0.0 && cost >= self.config.max_cost_usd {
            return Err(BudgetError::CostOverLimit(cost));
        }
        Ok(())
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.total_input_tokens.store(0, Ordering::Relaxed);
        self.total_output_tokens.store(0, Ordering::Relaxed);
        self.total_cost_millicents.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_tokens() {
        let tracker = BudgetTracker::new(BudgetConfig::default());
        tracker.add_input_tokens(100);
        tracker.add_output_tokens(50);
        assert_eq!(tracker.total_tokens(), 150);
    }

    #[test]
    fn tracks_cost() {
        let tracker = BudgetTracker::new(BudgetConfig::default());
        tracker.add_cost(0.25);
        assert!((tracker.total_cost_usd() - 0.25).abs() < 0.0001);
    }

    #[test]
    fn no_warning_when_under_threshold() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_context_tokens: 1000,
            warn_at: 0.8,
            max_cost_usd: 1.0,
        });
        assert!(tracker.warn_message().is_none());
    }

    #[test]
    fn warning_when_over_threshold() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_context_tokens: 100,
            warn_at: 0.8,
            max_cost_usd: 1.0,
        });
        tracker.add_input_tokens(90);
        tracker.add_output_tokens(10);
        let msg = tracker.warn_message();
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("tokens at 100%"));
    }

    #[test]
    fn over_limit_check() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_context_tokens: 50,
            warn_at: 0.8,
            max_cost_usd: 1.0,
        });
        assert!(!tracker.is_over_limit());
        tracker.add_input_tokens(60);
        assert!(tracker.is_over_limit());
    }

    #[test]
    fn over_limit_cost() {
        let tracker = BudgetTracker::new(BudgetConfig {
            max_context_tokens: 100_000,
            warn_at: 0.8,
            max_cost_usd: 0.10,
        });
        assert!(!tracker.is_over_limit());
        tracker.add_cost(0.15);
        assert!(tracker.is_over_limit());
    }

    #[test]
    fn reset_clears() {
        let tracker = BudgetTracker::new(BudgetConfig::default());
        tracker.add_input_tokens(999);
        tracker.add_cost(5.0);
        tracker.reset();
        assert_eq!(tracker.total_tokens(), 0);
        assert!((tracker.total_cost_usd()).abs() < 0.0001);
    }
}
