//! Terminal diagram renderer.
//!
//! Converts common Mermaid diagram types to terminal-optimized
//! ASCII output. Supports flowcharts, sequence diagrams, and
//! simple class diagrams.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiagramError {
    #[error("Unsupported diagram type: {0}")]
    UnsupportedType(String),

    #[error("Empty diagram source")]
    EmptySource,
}

/// Detect the Mermaid diagram type from source.
fn detect_type(source: &str) -> Result<&str, DiagramError> {
    let first_line = source.lines().next().ok_or(DiagramError::EmptySource)?;
    let first_line = first_line.trim();

    if first_line.starts_with("flowchart") || first_line.starts_with("graph ") {
        Ok("flowchart")
    } else if first_line.starts_with("sequenceDiagram") {
        Ok("sequence")
    } else if first_line.starts_with("classDiagram") {
        Ok("class")
    } else {
        Err(DiagramError::UnsupportedType(first_line.to_string()))
    }
}

/// Render a Mermaid flowchart to terminal ASCII.
fn render_flowchart(source: &str) -> String {
    let mut output = String::new();
    output.push_str("┌─ Flowchart ──────────────────────┐\n");

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("flowchart") || line.starts_with("graph ") || line.starts_with("%%") {
            continue;
        }

        // Skip style directives
        if line.starts_with("style ") {
            continue;
        }

        // Detect subgraph
        if line.starts_with("subgraph ") {
            let title = line.trim_start_matches("subgraph ").trim_matches('"');
            output.push_str(&format!("  ├─ [{}]\n", title));
            continue;
        }
        if line.starts_with("end") {
            continue;
        }

        // Connections: A --> B or A ---> B or A -.-> B or A ==>

        if let Some(rest) = line.strip_suffix("-->|") {
            if rest.contains("-->") {
                let parts: Vec<&str> = line.split("-->").collect();
                if parts.len() >= 2 {
                    let (left, right_fragment) = (parts[0].trim(), parts[1].trim());
                    // Check for label: right_fragment might be |label| dest
                    if right_fragment.starts_with('|') {
                        if let Some((label, dest)) = right_fragment[1..].split_once('|') {
                            output.push_str(&format!(
                                "  {} ──({})──→ {}\n",
                                left, label, dest.trim()
                            ));
                        }
                    } else {
                        output.push_str(&format!("  {} → {}\n", left, right_fragment));
                    }
                }
            } else {
                output.push_str(&format!("  {}\n", line));
            }
        } else if line.contains("-->") || line.contains("---") {
            let arrow = if line.contains("==>") { "══>" } else { "──>" };
            let parts: Vec<&str> = if line.contains("==") {
                line.split("==>").collect()
            } else {
                line.split("-->").collect()
            };
            if parts.len() >= 2 {
                output.push_str(&format!("  {} {} {}\n", parts[0].trim(), arrow, parts[1].trim()));
            }
        } else if line.contains("->>") {
            let parts: Vec<&str> = line.split("->>").collect();
            if parts.len() >= 2 {
                output.push_str(&format!("  {} ───→ {}\n", parts[0].trim(), parts[1].trim()));
            }
        }
    }

    output.push_str("└──────────────────────────────────┘\n");
    output
}

/// Render a sequence diagram to terminal ASCII.
fn render_sequence(source: &str) -> String {
    let mut output = String::new();
    output.push_str("┌─ Sequence Diagram ────────────────┐\n");

    let mut participants: Vec<String> = Vec::new();
    let mut messages: Vec<String> = Vec::new();

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("sequenceDiagram") || line.starts_with("%%") {
            continue;
        }

        if line.starts_with("participant ") {
            let name = line.trim_start_matches("participant ").trim();
            participants.push(name.to_string());
            continue;
        }

        if line.starts_with("actor ") {
            let name = line.trim_start_matches("actor ").trim();
            participants.push(format!("[{}]", name));
            continue;
        }

        // Messages: A->>B: text, A-->>B: text, A-xB: text
        if let Some((rest, msg)) = line.split_once(": ") {
            let (from, arrow, to) = if rest.contains("->>") {
                let parts: Vec<&str> = rest.split("->>").collect();
                (parts[0].trim(), "──→", parts[1].trim())
            } else if rest.contains("-->>") {
                let parts: Vec<&str> = rest.split("-->>").collect();
                (parts[0].trim(), "···→", parts[1].trim())
            } else if rest.contains("-x") {
                let parts: Vec<&str> = rest.split("-x").collect();
                (parts[0].trim(), "──✗", parts[1].trim())
            } else if rest.contains("-->") {
                let parts: Vec<&str> = rest.split("-->").collect();
                (parts[0].trim(), "···→", parts[1].trim())
            } else {
                continue;
            };

            let from_short = shorten_name(from, 6);
            let to_short = shorten_name(to, 6);
            let msg_short = if msg.len() > 30 { format!("{}...", &msg[..27]) } else { msg.to_string() };

            messages.push(format!("  {} {} {} : {}", from_short, arrow, to_short, msg_short));
        }
    }

    // Print participant headers
    if participants.is_empty() && messages.is_empty() {
        // Try to extract participants from message lines
        output.push_str("  (no participants defined)\n");
    } else {
        output.push_str(&format!("  Participants: {}\n", participants.join(", ")));
        output.push_str("  ──────────────────────────────────\n");
        for msg in &messages {
            output.push_str(msg);
            output.push('\n');
        }
    }

    output.push_str("└──────────────────────────────────┘\n");
    output
}

fn shorten_name(name: &str, max: usize) -> String {
    if name.len() <= max {
        name.to_string()
    } else {
        format!("{}..", &name[..max.saturating_sub(2)])
    }
}

/// Render a Mermaid source string to terminal-optimized ASCII.
pub fn render(source: &str) -> Result<String, DiagramError> {
    let source = source.trim();
    if source.is_empty() {
        return Err(DiagramError::EmptySource);
    }

    match detect_type(source)? {
        "flowchart" => Ok(render_flowchart(source)),
        "sequence" => Ok(render_sequence(source)),
        "class" => Ok("┌─ Class Diagram ───────────────────┐\n  (class diagram rendering not yet implemented)\n└──────────────────────────────────┘\n".into()),
        t => Err(DiagramError::UnsupportedType(t.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_flowchart() {
        let source = "flowchart LR\n  A --> B\n  B --> C\n";
        let result = render(source);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Flowchart"));
    }

    #[test]
    fn detect_sequence() {
        let source = "sequenceDiagram\n  participant A\n  A->>B: Hello\n";
        let result = render(source);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Sequence Diagram"));
    }

    #[test]
    fn empty_source_errors() {
        let result = render("");
        assert!(result.is_err());
        matches!(result, Err(DiagramError::EmptySource));
    }

    #[test]
    fn unsupported_type_errors() {
        let result = render("gantt\n  title A Gantt Chart\n");
        assert!(result.is_err());
    }

    #[test]
    fn flowchart_includes_nodes() {
        let source = "flowchart LR\n  A --> B\n  B --> C\n";
        let output = render(source).unwrap();
        assert!(output.contains("A"));
        assert!(output.contains("B"));
        assert!(output.contains("C"));
    }

    #[test]
    fn sequence_includes_messages() {
        let source = r#"sequenceDiagram
            participant User
            participant System
            User->>System: Login request
            System-->>User: Token"#;
        let output = render(source).unwrap();
        assert!(output.contains("Login request"));
        assert!(output.contains("Token"));
    }
}
