//! Sub-agent definitions — loads and manages agent .md files.
//!
//! Agents are defined in `~/.ante/agents/` as Markdown files with
//! YAML frontmatter specifying name, description, prompt, tools,
//! model, and max_turns.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// A loaded sub-agent definition.
#[derive(Debug, Clone)]
pub struct SubAgent {
    /// Agent identifier (filename stem).
    pub id: String,
    /// Display name from frontmatter.
    pub name: String,
    /// Description for task routing.
    pub description: String,
    /// System prompt / instructions.
    pub prompt: String,
    /// Tools this agent can access.
    pub tools: Vec<String>,
    /// Model override.
    pub model: Option<String>,
    /// Maximum interaction turns.
    pub max_turns: u32,
    /// File path of the agent definition.
    pub path: PathBuf,
}

/// YAML frontmatter parsed from an agent .md file.
#[derive(Debug, Default, Deserialize)]
struct AgentFrontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    max_turns: u32,
}

/// Task dependency graph node.
#[derive(Debug, Clone)]
pub struct TaskNode {
    pub id: String,
    pub description: String,
    pub assigned_agent: Option<String>,
    pub dependencies: Vec<String>, // task IDs that must complete first
}

/// Decomposition result — a DAG of tasks with assigned agents.
#[derive(Debug, Clone)]
pub struct TaskGraph {
    pub tasks: Vec<TaskNode>,
}

/// Errors from agent operations.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("Failed to read agent directory {0}: {1}")]
    ReadDir(PathBuf, std::io::Error),

    #[error("Failed to parse agent file {0}: {1}")]
    ParseFile(PathBuf, String),

    #[error("No suitable agent found for task: {0}")]
    NoSuitableAgent(String),

    #[error("Agent '{0}' not found")]
    NotFound(String),
}

/// Loads and manages sub-agent definitions.
pub struct AgentRegistry {
    agents: Vec<SubAgent>,
}

impl AgentRegistry {
    /// Create a new AgentRegistry by scanning the given directory.
    pub fn load(agent_dir: &Path) -> Result<Self, AgentError> {
        if !agent_dir.exists() {
            return Ok(AgentRegistry {
                agents: Vec::new(),
            });
        }

        let mut agents = Vec::new();
        let entries = fs::read_dir(agent_dir)
            .map_err(|e| AgentError::ReadDir(agent_dir.to_path_buf(), e))?;

        for entry in entries {
            let entry = entry.map_err(|e| AgentError::ReadDir(agent_dir.to_path_buf(), e))?;
            let path = entry.path();
            if path.extension().map_or(true, |ext| ext != "md") {
                continue;
            }

            match parse_agent_file(&path) {
                Ok(agent) => agents.push(agent),
                Err(e) => {
                    // Log but continue — don't fail on bad files
                    eprintln!("[ante] Warning: failed to parse agent {:?}: {}", path, e);
                }
            }
        }

        Ok(AgentRegistry { agents })
    }

    /// Find the best-matching agent for a task description.
    pub fn find_best_match(&self, task_description: &str) -> Option<&SubAgent> {
        let lower = task_description.to_lowercase();
        let mut scored: Vec<(&SubAgent, usize)> = self
            .agents
            .iter()
            .map(|a| {
                let score = similarity_score(&lower, &a.name.to_lowercase(), &a.description.to_lowercase());
                (a, score)
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));

        scored.into_iter().next().and_then(|(a, s)| if s > 0 { Some(a) } else { None })
    }

    /// Get a specific agent by name.
    pub fn get(&self, name: &str) -> Option<&SubAgent> {
        self.agents.iter().find(|a| a.name == name || a.id == name)
    }

    /// All loaded agents.
    pub fn all(&self) -> &[SubAgent] {
        &self.agents
    }

    /// Number of loaded agents.
    pub fn count(&self) -> usize {
        self.agents.len()
    }
}

/// Parse a Markdown file with YAML frontmatter into a SubAgent.
fn parse_agent_file(path: &Path) -> Result<SubAgent, AgentError> {
    let content = fs::read_to_string(path)
        .map_err(|e| AgentError::ParseFile(path.to_path_buf(), e.to_string()))?;

    let (frontmatter, _body) = parse_frontmatter(&content)
        .map_err(|e| AgentError::ParseFile(path.to_path_buf(), e))?;

    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    Ok(SubAgent {
        id,
        name: frontmatter.name,
        description: frontmatter.description,
        prompt: frontmatter.prompt,
        tools: frontmatter.tools,
        model: frontmatter.model,
        max_turns: if frontmatter.max_turns == 0 { 10 } else { frontmatter.max_turns },
        path: path.to_path_buf(),
    })
}

/// Parse YAML frontmatter from a Markdown string.
fn parse_frontmatter(content: &str) -> Result<(AgentFrontmatter, String), String> {
    let content = content.trim();
    if !content.starts_with("---") {
        // No frontmatter — return default
        return Ok((
            AgentFrontmatter::default(),
            content.to_string(),
        ));
    }

    // Find closing ---
    let end = content[3..]
        .find("\n---")
        .map(|pos| pos + 3)
        .ok_or("Unclosed frontmatter delimiter")?;

    let yaml_str = &content[3..end];
    let body = content[end + 4..].trim().to_string();

    // Parse YAML frontmatter (basic key-value parser to avoid serde_yaml dep)
    let frontmatter = parse_yaml_kv(yaml_str)?;

    Ok((frontmatter, body))
}

/// Minimal key-value YAML parser (avoids serde_yaml dependency).
fn parse_yaml_kv(yaml_str: &str) -> Result<AgentFrontmatter, String> {
    let mut fm = AgentFrontmatter::default();

    for line in yaml_str.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_lowercase();
            let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
            match key.as_str() {
                "name" => fm.name = value,
                "description" => fm.description = value,
                "prompt" => fm.prompt = value,
                "model" => fm.model = Some(value),
                "max_turns" => {
                    fm.max_turns = value.parse().unwrap_or(10);
                }
                "tools" => {
                    fm.tools = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {} // skip unknown keys
            }
        }
    }

    Ok(fm)
}

/// Simple keyword-overlap similarity score between task and agent.
fn similarity_score(task_lower: &str, name_lower: &str, desc_lower: &str) -> usize {
    let task_words: Vec<String> = task_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();

    let combined = format!("{} {}", name_lower, desc_lower);
    let agent_words: Vec<String> = combined
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_string())
        .collect();

    task_words
        .iter()
        .filter(|tw| agent_words.iter().any(|aw| aw.contains(tw.as_str()) || tw.contains(aw.as_str())))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_agent_file(dir: &Path, content: &str) -> PathBuf {
        // Derive name from YAML frontmatter for unique filenames
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);

        let name = content
            .lines()
            .find(|l| l.starts_with("name:"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| {
                let trimmed = s.trim().trim_matches('"').to_lowercase();
                let n = COUNTER.fetch_add(1, Ordering::Relaxed);
                format!("{}-{}", trimmed, n)
            })
            .unwrap_or_else(|| {
                let n = COUNTER.fetch_add(1, Ordering::Relaxed);
                format!("agent-{}", n)
            });

        let path = dir.join(format!("{}.md", name));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    #[test]
    fn parses_frontmatter_with_all_fields() {
        let md = r#"---
name: "File Reader"
description: "Reads and analyzes files"
prompt: "You are a file reader assistant."
tools: "Bash,Read"
model: "claude-sonnet-4"
max_turns: 15
---
# File Reader Agent

Your task is to read files.
"#;

        let tmp = tempfile::tempdir().unwrap();
        let path = write_agent_file(tmp.path(), md);
        let agent = parse_agent_file(&path).unwrap();

        assert_eq!(agent.name, "File Reader");
        assert_eq!(agent.description, "Reads and analyzes files");
        assert_eq!(agent.prompt, "You are a file reader assistant.");
        assert_eq!(agent.tools.len(), 2);
        assert_eq!(agent.model.as_deref(), Some("claude-sonnet-4"));
        assert_eq!(agent.max_turns, 15);
    }

    #[test]
    fn agent_without_frontmatter_defaults() {
        let md = "# Plain Agent\n\nJust some markdown.";
        let tmp = tempfile::tempdir().unwrap();
        let path = write_agent_file(tmp.path(), md);
        let agent = parse_agent_file(&path).unwrap();

        assert_eq!(agent.name, "");
        assert_eq!(agent.max_turns, 10); // default
        assert!(agent.model.is_none());
    }

    #[test]
    fn registry_loads_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        write_agent_file(
            &agents_dir,
            "---\nname: Reader\ndescription: reads things\n---",
        );
        write_agent_file(
            &agents_dir,
            "---\nname: Writer\ndescription: writes things\n---",
        );

        let registry = AgentRegistry::load(&agents_dir).unwrap();
        assert_eq!(registry.count(), 2);
    }

    #[test]
    fn find_best_match_ranks_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        write_agent_file(
            &agents_dir,
            "---\nname: FileReader\ndescription: reads files and directories\n---",
        );
        write_agent_file(
            &agents_dir,
            "---\nname: CodeWriter\ndescription: writes code and modifies files\n---",
        );

        let registry = AgentRegistry::load(&agents_dir).unwrap();

        // Should prefer FileReader for file-reading tasks
        let best = registry.find_best_match("I need to read a directory");
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "FileReader");

        // Should prefer CodeWriter for writing tasks
        let best = registry.find_best_match("write some code");
        assert!(best.is_some());
        assert_eq!(best.unwrap().name, "CodeWriter");
    }

    #[test]
    fn empty_registry_returns_none() {
        let registry = AgentRegistry::load(Path::new("/nonexistent")).unwrap();
        assert_eq!(registry.count(), 0);
        assert!(registry.find_best_match("anything").is_none());
    }
}
