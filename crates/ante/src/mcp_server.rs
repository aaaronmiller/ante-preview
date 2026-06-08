//! Lightweight MCP server that provides built-in Ante tools (diagram, todo, memory).
//!
//! Runs as a child process of the main Ante agent and communicates
//! via newline-delimited JSON-RPC 2.0 over stdio.  Exposes:
//!
//! - `diagram`  — render a Mermaid source string to ASCII art
//! - `todo_add` — add a new todo item
//! - `todo_list` — list all todo items
//! - `todo_done` — mark a todo item as done
//! - `memory_add` — add a shared wiki-memory entry
//! - `memory_search` — search shared wiki-memory entries
//! - `memory_get_context` — retrieve project-scoped memory context

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use agent_sdk::memory::server::MemoryServer;
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
    let mut memory = MemoryServer::open(default_memory_db_path(), 20).ok();

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

        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

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
                handle_tools_call(id, &params, &mut todos, &mut memory)
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
            },
            {
                "name": "memory_add",
                "description": "Add an entry to Ante shared wiki-memory.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "content": {
                            "type": "string",
                            "description": "Memory content"
                        },
                        "tags": {
                            "type": "string",
                            "description": "Comma-separated tags"
                        },
                        "project": {
                            "type": "string",
                            "description": "Project scope"
                        }
                    },
                    "required": ["content"]
                }
            },
            {
                "name": "memory_search",
                "description": "Search Ante shared wiki-memory entries.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query"
                        }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "memory_get_context",
                "description": "Get recent Ante wiki-memory context for a project.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project scope"
                        },
                        "max": {
                            "type": "integer",
                            "description": "Maximum entries"
                        }
                    },
                    "required": ["project"]
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
    memory: &mut Option<MemoryServer>,
) -> String {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);

    match name {
        "diagram" => handle_diagram(id, &args),
        "todo_add" => handle_todo_add(id, &args, todos),
        "todo_list" => handle_todo_list(id, todos),
        "todo_done" => handle_todo_done(id, &args, todos),
        "memory_add" => handle_memory_add(id, &args, memory),
        "memory_search" => handle_memory_search(id, &args, memory),
        "memory_get_context" => handle_memory_get_context(id, &args, memory),
        _ => make_error(id, -32602, &format!("Unknown tool: {name}")),
    }
}

fn default_memory_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("ANTE_MEMORY_DB") {
        return expand_tilde(path);
    }
    if let Ok(dir) = std::env::var("AI_WIKI_DIR") {
        return expand_tilde(dir).join(".meta").join("ante-memory.db");
    }
    if let Ok(home) = std::env::var("HOME") {
        let wiki_memory = PathBuf::from(&home).join("code").join("wiki-memory");
        if wiki_memory.exists() {
            return wiki_memory
                .join("wiki")
                .join(".meta")
                .join("ante-memory.db");
        }
        return PathBuf::from(home)
            .join("ai-wiki")
            .join(".meta")
            .join("ante-memory.db");
    }
    PathBuf::from("ai-wiki")
        .join(".meta")
        .join("ante-memory.db")
}

fn expand_tilde(path: String) -> PathBuf {
    if path == "~" {
        return std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
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

fn handle_memory_add(id: u64, args: &Value, memory: &mut Option<MemoryServer>) -> String {
    let content = match args.get("content").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return make_error(id, -32602, "Missing required argument: content"),
    };
    let tags = args.get("tags").and_then(|s| s.as_str()).unwrap_or("mcp");
    let project = args
        .get("project")
        .and_then(|s| s.as_str())
        .unwrap_or("default");

    match memory
        .as_mut()
        .map(|m| m.add_memory(content.to_string(), tags.to_string(), project.to_string()))
    {
        Some(Ok(entry)) => {
            let result = serde_json::json!({
                "content": [{"type": "text", "text": format!("Added memory: {}", entry.id)}]
            });
            make_result(id, result)
        }
        Some(Err(e)) => memory_error_result(id, &e),
        None => memory_error_result(id, "Memory server not available"),
    }
}

fn handle_memory_search(id: u64, args: &Value, memory: &mut Option<MemoryServer>) -> String {
    let query = match args.get("query").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return make_error(id, -32602, "Missing required argument: query"),
    };

    match memory.as_ref() {
        Some(memory) => {
            let results = memory.search(query);
            make_result(
                id,
                serde_json::json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())}]
                }),
            )
        }
        None => memory_error_result(id, "Memory server not available"),
    }
}

fn handle_memory_get_context(id: u64, args: &Value, memory: &mut Option<MemoryServer>) -> String {
    let project = match args.get("project").and_then(|s| s.as_str()) {
        Some(s) => s,
        None => return make_error(id, -32602, "Missing required argument: project"),
    };
    let max = args.get("max").and_then(|m| m.as_u64()).unwrap_or(10) as usize;

    match memory.as_ref() {
        Some(memory) => {
            let results = memory.get_context(project, max);
            make_result(
                id,
                serde_json::json!({
                    "content": [{"type": "text", "text": serde_json::to_string_pretty(&results).unwrap_or_else(|_| "[]".to_string())}]
                }),
            )
        }
        None => memory_error_result(id, "Memory server not available"),
    }
}

fn memory_error_result(id: u64, message: &str) -> String {
    make_result(
        id,
        serde_json::json!({
            "content": [{"type": "text", "text": format!("Memory error: {message}")}],
            "isError": true
        }),
    )
}
