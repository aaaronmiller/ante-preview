//! Lightweight MCP server that provides built-in Ante tools (diagram, todo).
//!
//! Runs as a child process of the main Ante agent and communicates
//! via newline-delimited JSON-RPC 2.0 over stdio.  Exposes:
//!
//! - `diagram`  — render a Mermaid source string to ASCII art
//! - `todo_add` — add a new todo item
//! - `todo_list` — list all todo items
//! - `todo_done` — mark a todo item as done

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use agent_sdk::ui::diagram;
use agent_sdk::ui::todo::TodoList;
use serde_json::Value;

/// Start the MCP server loop.  Reads JSON-RPC requests from stdin
/// and writes responses to stdout.  Returns on EOF (parent closed pipe).
pub fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    // ── Open per-session todo store ────────────────────────────────
    let ante_dir = std::env::var("HOME")
        .map(|h| PathBuf::from(h).join(".ante"))
        .unwrap_or_else(|_| PathBuf::from(".ante"));
    let todo_path = ante_dir.join("todo.json");
    let mut todos = TodoList::open(todo_path).ok();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut next_id: u64 = 0;

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = make_error(0, -32700, &format!("Parse error: {e}"));
                let mut out = stdout.lock();
                let _ = writeln!(out, "{err_resp}");
                let _ = out.flush();
                continue;
            }
        };

        let method = request
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        let id = request
            .get("id")
            .and_then(|i| i.as_u64())
            .unwrap_or_else(|| {
                next_id += 1;
                next_id
            });

        let response = match method {
            "initialize" => handle_initialize(id),
            "tools/list" => handle_tools_list(id),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(Value::Null);
                handle_tools_call(id, &params, &mut todos)
            }
            "notifications/initialized" => continue, // no response
            _ => make_error(id, -32601, &format!("Method not found: {method}")),
        };

        let mut out = stdout.lock();
        let _ = writeln!(out, "{response}");
        let _ = out.flush();
    }

    Ok(())
}

// ─── MCP Response builders ──────────────────────────────────────────────────

fn make_result(id: u64, result: Value) -> String {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    });
    serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string())
}

fn make_error(id: u64, code: i32, message: &str) -> String {
    let resp = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
        }
    });
    serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string())
}

// ─── Handlers ───────────────────────────────────────────────────────────────

fn handle_initialize(id: u64) -> String {
    let result = serde_json::json!({
        "protocolVersion": "2025-03-26",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "ante-tools",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    make_result(id, result)
}

fn handle_tools_list(id: u64) -> String {
    let tools = serde_json::json!({
        "tools": [
            {
                "name": "diagram",
                "description": "Render a Mermaid diagram source string to ASCII art. Supports flowchart, sequenceDiagram, and classDiagram types.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": {
                            "type": "string",
                            "description": "Mermaid diagram source code"
                        }
                    },
                    "required": ["source"]
                }
            },
            {
                "name": "todo_add",
                "description": "Add a new todo item to the todo list.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": {
                            "type": "string",
                            "description": "Text of the todo item"
                        }
                    },
                    "required": ["text"]
                }
            },
            {
                "name": "todo_list",
                "description": "List all todo items, including their status (done/pending).",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "todo_done",
                "description": "Mark a todo item as completed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "integer",
                            "description": "The ID of the todo item to mark as done"
                        }
                    },
                    "required": ["id"]
                }
            }
        ]
    });
    make_result(id, tools)
}

fn handle_tools_call(
    id: u64,
    params: &Value,
    todos: &mut Option<TodoList>,
) -> String {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);

    match name {
        "diagram" => handle_diagram(id, &args),
        "todo_add" => handle_todo_add(id, &args, todos),
        "todo_list" => handle_todo_list(id, todos),
        "todo_done" => handle_todo_done(id, &args, todos),
        _ => make_error(id, -32602, &format!("Unknown tool: {name}")),
    }
}

fn handle_diagram(id: u64, args: &Value) -> String {
    let source = match args.get("source").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return make_error(id, -32602, "Missing required argument: source"),
    };

    match diagram::render(source) {
        Ok(ascii) => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": ascii
                    }
                ]
            });
            make_result(id, result)
        }
        Err(e) => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Diagram error: {e}")
                    }
                ],
                "isError": true
            });
            make_result(id, result)
        }
    }
}

fn handle_todo_add(id: u64, args: &Value, todos: &mut Option<TodoList>) -> String {
    let text = match args.get("text").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return make_error(id, -32602, "Missing required argument: text"),
    };

    match todos.as_mut().and_then(|t| t.add(text).ok()) {
        Some(item) => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("✅ Todo #{}: {}", item.id, item.text)
                    }
                ]
            });
            make_result(id, result)
        }
        None => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "Error: Todo list not available"
                    }
                ],
                "isError": true
            });
            make_result(id, result)
        }
    }
}

fn handle_todo_list(id: u64, todos: &mut Option<TodoList>) -> String {
    let items = match todos.as_ref() {
        Some(t) => t.list(),
        None => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": "Error: Todo list not available"
                    }
                ],
                "isError": true
            });
            return make_result(id, result);
        }
    };

    if items.is_empty() {
        let result = serde_json::json!({
            "content": [
                {
                    "type": "text",
                    "text": "No todos."
                }
            ]
        });
        return make_result(id, result);
    }

    let mut lines = vec!["📋 **Todo List**:".to_string()];
    for item in items {
        let status = if item.done { "✅" } else { "⬜" };
        lines.push(format!("  {status} #{} {}", item.id, item.text));
    }

    let result = serde_json::json!({
        "content": [
            {
                "type": "text",
                "text": lines.join("\n")
            }
        ]
    });
    make_result(id, result)
}

fn handle_todo_done(id: u64, args: &Value, todos: &mut Option<TodoList>) -> String {
    let todo_id = match args.get("id").and_then(|i| i.as_u64()) {
        Some(i) => i as usize,
        None => return make_error(id, -32602, "Missing required argument: id"),
    };

    match todos.as_mut().and_then(|t| t.complete(todo_id).ok()) {
        Some(item) => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("✅ Completed: {}", item.text)
                    }
                ]
            });
            make_result(id, result)
        }
        None => {
            let result = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Error: Todo #{} not found or already completed", todo_id)
                    }
                ],
                "isError": true
            });
            make_result(id, result)
        }
    }
}
