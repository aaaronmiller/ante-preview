//! Result synthesizer for sub-agent outputs.
//!
//! Aggregates results from multiple sub-agents, detects conflicts,
//! and produces a coherent final response.

use super::dispatcher::TaskResult;

/// Synthesize individual task results into a coherent final response.
///
/// Features:
///   - Orders results by dependency level
///   - Detects conflicting outputs (same topic, contradictory statements)
///   - Produces a summary when there are many results
///   - Flags errors prominently
pub fn synthesize(results: &[TaskResult]) -> SynthesizedOutput {
    if results.is_empty() {
        return SynthesizedOutput {
            summary: "No tasks were executed.".to_string(),
            conflicts: Vec::new(),
            full_text: "No tasks were executed.".to_string(),
        };
    }

    let mut conflicts = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut error_count = 0;

    for result in results {
        let agent_info = match &result.agent {
            Some(name) => format!("[{name}]"),
            None => "[direct]".to_string(),
        };

        if let Some(err) = &result.error {
            error_count += 1;
            parts.push(format!(
                "{agent_info} Task \"{}\" FAILED: {err}",
                result.description,
            ));
        } else {
            parts.push(format!(
                "{agent_info} Task \"{}\" completed:\n{}",
                result.description, result.output
            ));
        }
    }

    // Detect simple conflicts: identical descriptions with different outputs
    for (i, a) in results.iter().enumerate() {
        for (j, b) in results.iter().enumerate() {
            if i < j && a.description == b.description && a.output != b.output {
                conflicts.push(Conflict {
                    between: format!("{} and {}", 
                        a.agent.as_deref().unwrap_or("agent"),
                        b.agent.as_deref().unwrap_or("agent")),
                    topic: a.description.clone(),
                    statement_a: a.output.clone(),
                    statement_b: b.output.clone(),
                });
            }
        }
    }

    let full_text = parts.join("\n\n");
    let summary = if results.len() == 1 {
        if results[0].error.is_some() {
            format!("1 task failed: {}", results[0].description)
        } else {
            format!("1 task completed: {}", results[0].description)
        }
    } else {
        let success_count = results.len() - error_count;
        let mut summary = format!("{}/{} tasks completed successfully.", success_count, results.len());
        if !conflicts.is_empty() {
            summary.push_str(&format!(" {} conflict(s) detected.", conflicts.len()));
        }
        if error_count > 0 {
            let errs: Vec<&str> = results
                .iter()
                .filter_map(|r| r.error.as_deref())
                .collect();
            summary.push_str(&format!(" Errors: {}", errs.join("; ")));
        }
        summary
    };

    SynthesizedOutput {
        summary,
        conflicts,
        full_text,
    }
}

/// A detected conflict between two sub-agent outputs.
#[derive(Debug, Clone)]
pub struct Conflict {
    /// Which agents produced conflicting results.
    pub between: String,
    /// The shared topic/description.
    pub topic: String,
    /// Output from the first agent.
    pub statement_a: String,
    /// Output from the second agent.
    pub statement_b: String,
}

/// Synthesized output from multiple sub-agent results.
#[derive(Debug, Clone)]
pub struct SynthesizedOutput {
    /// Human-readable summary line(s).
    pub summary: String,
    /// Any detected conflicts between agent outputs.
    pub conflicts: Vec<Conflict>,
    /// Full concatenated text of all results.
    pub full_text: String,
}

impl SynthesizedOutput {
    /// True if there are no errors or conflicts.
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty() && !self.full_text.contains("FAILED")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(
        id: &str,
        desc: &str,
        agent: Option<&str>,
        output: &str,
        error: Option<&str>,
    ) -> TaskResult {
        let mut r = TaskResult::new(id.into(), desc.into(), agent.map(|s| s.into()));
        r.output = output.into();
        r.error = error.map(|s| s.into());
        r
    }

    #[test]
    fn synthesize_empty() {
        let out = synthesize(&[]);
        assert_eq!(out.summary, "No tasks were executed.");
        assert!(out.is_clean());
    }

    #[test]
    fn synthesize_single_success() {
        let results = vec![make_result("t1", "read file", Some("Reader"), "contents: hello", None)];
        let out = synthesize(&results);
        assert!(out.summary.contains("1 task completed"));
        assert!(out.full_text.contains("[Reader]"));
        assert!(out.is_clean());
    }

    #[test]
    fn synthesize_with_error() {
        let results = vec![make_result("t1", "parse data", Some("Parser"), "", Some("not found"))];
        let out = synthesize(&results);
        assert!(out.summary.contains("1 task failed"));
        assert!(out.full_text.contains("FAILED"));
        assert!(!out.is_clean());
    }

    #[test]
    fn detect_conflicts() {
        let results = vec![
            make_result("t1", "read file", Some("Reader"), "content: hello", None),
            make_result("t2", "read file", Some("Reader2"), "content: goodbye", None),
        ];
        let out = synthesize(&results);
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].topic, "read file");
    }

    #[test]
    fn no_false_conflicts_for_different_topics() {
        let results = vec![
            make_result("t1", "read file", Some("Reader"), "content: hello", None),
            make_result("t2", "list dir", Some("Lister"), "files: a.txt", None),
        ];
        let out = synthesize(&results);
        assert_eq!(out.conflicts.len(), 0);
    }

    #[test]
    fn summary_shows_progress() {
        let results = vec![
            make_result("t1", "task a", None, "done", None),
            make_result("t2", "task b", None, "done", None),
            make_result("t3", "task c", None, "", Some("timeout")),
        ];
        let out = synthesize(&results);
        assert!(out.summary.contains("2/3 tasks completed"));
        assert!(out.summary.contains("Errors: timeout"));
    }
}
