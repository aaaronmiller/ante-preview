# Ante — Extensible Agent Runtime

**Ante** is an extensible agent runtime wrapping Claude Code. It adds memory, hooks, tools, sessions, MCP servers, sub-agents, model routing, HITL approval, and wiki-memory to the upstream CLI — without losing any Claude Code capability.

## Differentiators (vs. Upstream Claude Code)

| Feature | Ante | Upstream Claude Code |
|---------|------|---------------------|
| **Session recording** | Always-on JSONL logging to `~/.ante/sessions/` — every exchange is recoverable | ❌ No session persistence |
| **Session recovery** | `ante --continue` restores prior session context; `ante --resume <id>` targets a specific session | ❌ |
| **Event hook system** | Bash / LLM / MCP-tool hooks on `PreToolUse`, `PostToolUse`, `PreCompact`, `SessionStart`, `SessionEnd`, and more | ❌ |
| **Wiki-memory integration** | All agents share `~/ai-wiki/.meta/ante-memory.db` — symlinked to the [wiki-memory](https://github.com/cheta/wiki-memory) project's dream agent when installed | ❌ |
| **Memory MCP server** | Embedded stdio MCP server (`memory_add`, `memory_search`, `memory_get_context` tools) in every session | ❌ |
| **Sub-agents** | YAML/markdown-defined agent registry with best-match dispatch | ❌ |
| **Model router** | Dynamic model selection per query based on capability scoring and cost | ❌ |
| **Human-in-the-loop (HITL)** | Configurable approval modes: per-request, batch-risk-threshold, or approve-all | ❌ |
| **Session management CLI** | `ante sessions list`, `ante sessions show <id>`, `ante sessions resume <id>` | ❌ |
| **Context budget tracking** | Token and cost budgets with configurable warning thresholds | ❌ |
| **Diagram rendering** | `ante diagram <mermaid-source>` — renders Mermaid to terminal ASCII | ❌ |
| **Sensitive tool blocklist** | Configurable Bash/Write/Execute blocklist with danger-pattern matching | ❌ |
| **Claude Code compatibility** | Reads `.claude/settings.json`, translates event names for cross-compat | ❌ (N/A) |

## Shared Features (Both Versions)

- Interactive REPL with Claude Code
- One-shot query mode (`ante query "..."`)
- File editing, Bash execution, web search via Claude Code
- Per-directory `.claude/` project settings
- MCP server support (configurable via `mcp_servers` in `settings.json`)

## Quick Start

```sh
# Clone and build
git clone <url> ante-spec
cd ante-spec
cargo build --release -p ante

# First-run setup (creates ~/.ante/ and shared wiki memory)
./target/release/ante init

# Start a fresh interactive session
./target/release/ante

# Continue the most recent session in this directory
./target/release/ante --continue

# Query once and exit
./target/release/ante query "explain this code"

# Check local readiness
./target/release/ante doctor
```

## Wiki-Memory Integration

Ante uses the wiki-memory repo when it is installed at `~/code/wiki-memory`; otherwise it falls back to `~/ai-wiki`.

### How it works

- `MemoryStore` data lives at `~/code/wiki-memory/wiki/.meta/ante-memory.db` when that repo exists
- Without wiki-memory, `MemoryStore` data lives at `~/ai-wiki/.meta/ante-memory.db`
- `ante init` creates missing wiki directories, but does not replace an existing `~/ai-wiki` directory
- The wiki-memory **dream agent** can process raw/ sources, consolidate observations, and auto-create skills from repeated patterns
- All agents (pi, ante, Claude Code, etc.) can share the same wiki-backed memory database — what one learns, all know
- The embedded MCP server exposes `memory_add`, `memory_search`, and `memory_get_context`
- wiki-memory hooks (`pre_compact.py`, `session_end.py`) are registered in `~/.ante/settings.json`

### Architecture

```
~/code/wiki-memory/         # <-- git repo, preferred when present
  └── wiki/                 # <-- actual data directory
      ├── pages/             #     auto-consolidated knowledge articles
      ├── raw/               #     source observations
      └── .meta/
          └── ante-memory.db # <-- Ante MemoryStore (JSON-backed)

~/.ante/settings.json        # legacy paths are normalized at runtime
```

> **No wiki-memory? No problem.** If the repo isn't present, `~/ai-wiki` is created as a plain directory; everything still works.

## Launch Modes

| Command | Mode | Description |
|---------|------|-------------|
| `ante` | **REPL** | Interactive session with Claude — full extensibility, session recording always on |
| `ante --continue` | REPL | Same as above, but recovers the most recent session for the current directory |
| `ante --resume <id>` | REPL | Same as above, but recovers a specific session by ID |
| `ante query "..."` | **Headless** | One-shot query with full tooling — also records session |
| `ante init` | Setup | Create `~/.ante/` directory structure, `~/ai-wiki/`, default hooks |
| `ante memory add/search/context` | Direct | Direct memory operations (bypasses MCP server) |
| `ante todo add/list/done/clear` | Direct | Direct todo list operations |
| `ante sessions list/show/resume` | Direct | Session management (list, inspect, get resume command) |
| `ante agents list/run` | Direct | Sub-agent registry and dispatch |
| `ante doctor` | Direct | Readiness check for CLI tools, settings, parseable agents, hooks, internal MCP tools, sessions, and wiki-memory |
| `ante diagram <mermaid>` | Direct | Render Mermaid to terminal ASCII |

## All Arguments

### Global (REPL mode — `ante`)

| Argument | Description |
|----------|-------------|
| `--resume <id>` | Resume a specific session by ID |
| `-c`, `--continue` | Recover the most recent session for the current directory |
| `--model <name>` | Model override (e.g. `claude-sonnet-4-5`) |
| `--no-memory` | Skip memory context injection |
| `--no-hitl` | Disable human-in-the-loop approval |
| `--hitl-mode <mode>` | HITL mode: `per-request`, `batch-risk-threshold`, `approve-all` |
| `--risk-threshold <level>` | Auto-approval ceiling: `safe`, `low`, `medium`, `high`, `critical` |
| `--no-router` | Disable dynamic model routing (uses Claude's default) |
| `--cli-path <path>` | Path to Claude CLI binary |
| `-h`, `--help` | Print help |
| `-V`, `--version` | Print version |

### Query mode (`ante query`)

| Argument | Description |
|----------|-------------|
| `<prompt>` | Query text (positional, can be multiple words) |
| `--model <name>` | Model override |
| `--no-memory` | Skip memory context injection |
| `--no-hitl` | Disable HITL |
| `--hitl-mode <mode>` | HITL mode |
| `--risk-threshold <level>` | Risk threshold |
| `--no-router` | Disable model routing |

### Init mode (`ante init`)

| Argument | Description |
|----------|-------------|
| `--force` | Re-initialize (overwrites existing `settings.json`) |

### Memory mode (`ante memory`)

| Subcommand | Arguments | Description |
|-----------|-----------|-------------|
| `add` | `<text>`, `--tags <tags>`, `--project <name>` | Store a memory |
| `search` | `--query <text>` | Search stored memories |
| `context` | `--project <name>`, `--max <count>` | Get context for a project |

### Sessions mode (`ante sessions`)

| Subcommand | Arguments | Description |
|-----------|-----------|-------------|
| `list` | `--project <name>` | List sessions (optionally filtered by project) |
| `show` | `--id <session-id>`, `--messages <N>` | Inspect a session (last N messages) |
| `resume` | `--id <session-id>` | Print the `ante --resume` command |

### Todo mode (`ante todo`)

| Subcommand | Arguments | Description |
|-----------|-----------|-------------|
| `add` | `<text>` | Add a todo item |
| `list` | — | List all items |
| `done` | `--id <N>` | Mark item complete |
| `clear` | — | Clear completed items |

### Agents mode (`ante agents`)

| Subcommand | Arguments | Description |
|-----------|-----------|-------------|
| `list` | — | List registered sub-agents |
| `match` | `<task>` | Find best-match agent for a task without executing |
| `run` | `<task>`, `--backend <opencode|dry-run>`, `--model <provider/model>`, `--agent-dir <path>`, `--cwd <path>`, `--output <path>`, `--dry-run`, `--read-only`, `--skip-permissions` | Run the best matching agent through OpenCode or render the execution plan. When `--output run.md` is used, Ante also writes `run.json` telemetry. |

### Diagram mode (`ante diagram`)

| Argument | Description |
|----------|-------------|
| `<mermaid-source>` | Mermaid source text to render |

## Session System

Sessions are recorded automatically in **every** mode (REPL and query). No opt-in needed.

### Storage

```
~/.ante/sessions/
  ├── YYYY-MM-DD-HHMMSS-<safe-path>/
  │   ├── session.jsonl          # JSONL session log
  │   └── transcript.md          # Human-readable transcript
  └── sessions_index.json        # Fast-lookup index
```

### Format

Each line in `session.jsonl` is a self-describing JSON object with a `"type"` discriminator:

```jsonl
{"type":"session","sessionId":"2026-06-05-143022-...","cwd":"/home/user/project","modelId":"claude-sonnet-4-5","provider":"anthropic","timestamp":"2026-06-05T14:30:22+00:00"}
{"type":"message","messageId":"msg-...","message":{"role":"user","content":"explain this code"}}
{"type":"message","messageId":"msg-...","message":{"role":"assistant","content":...}}
{"type":"message","messageId":"msg-...","message":{"role":"tool_result","content":...}}
{"type":"model_change","fromModel":"...","toModel":"..."}
{"type":"thinking_level_change","fromLevel":"normal","toLevel":"high"}
```

This format is compatible with [coding_agent_session_search (cass)](https://github.com/cheta/coding_agent_session_search) — the `PiAgentConnector` reads these files directly.

### CLI Commands

```sh
# List all sessions
ante sessions list

# List sessions for the current project
ante sessions list --project $(basename $PWD)

# Inspect a session (last 10 messages)
ante sessions show --id 2026-06-05-143022-... --messages 10

# Get the resume command for a session
ante sessions resume --id 2026-06-05-143022-...
```

## Hook System

Ante fires events throughout the agent lifecycle. Hooks can be shell commands, LLM prompts, MCP tool calls, or sub-agent dispatches.

### Events

| Event | Fires When |
|-------|------------|
| `SessionStart` | Session begins |
| `SessionEnd` | Session ends |
| `PreUserPromptSubmit` | Before user prompt is sent to Claude |
| `PostUserPromptSubmit` | After user prompt is sent |
| `PreToolUse` | Before a tool is invoked (blocking supported) |
| `PostToolUse` | After a tool returns |
| `PreCompact` | Before context compaction |
| `PostCompact` | After context compaction |

### Default Hooks

Shipped with `ante init`:

1. **Blocklist** (`block-danger.sh`) — blocks dangerous Bash commands matching blocklist patterns. Fires on `PreToolUse` for `Bash*` tools.
2. **Pre-compact logger** (`pre_compact.py`) — logs memory and CPU usage before compaction. Fires on `PreCompact`.
3. **Session-end logger** (`session_end.py`) — captures a session summary on exit. Fires on `SessionEnd`.

Hook scripts live in `~/.ante/hooks/`. Edit them freely.

## MCP Server Configuration

Configure external MCP servers in `~/.ante/settings.json`:

```json
{
  "mcpServers": [
    {
      "name": "filesystem",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "."],
      "autoStart": true
    },
    {
      "name": "time",
      "command": "uvx",
      "args": ["mcp-server-time"],
      "lifecycle": "lazy"
    }
  ]
}
```

The embedded **memory MCP server** is started automatically in every session, exposing `memory_add`, `memory_search`, and `memory_get_context` tools.

## Model Router

Ante includes a dynamic model router that selects the best model for each query:

```json
{
  "modelPool": [
    {
      "name": "Claude Sonnet",
      "provider": "anthropic",
      "modelId": "claude-sonnet-4-5",
      "capabilityScore": 85,
      "costPer1kInput": 0.003,
      "costPer1kOutput": 0.015,
      "tags": ["fast", "reasoning"]
    },
    {
      "name": "Claude Haiku",
      "provider": "anthropic",
      "modelId": "claude-haiku-3-5",
      "capabilityScore": 60,
      "costPer1kInput": 0.0008,
      "costPer1kOutput": 0.004,
      "tags": ["fast", "cheap"]
    }
  ]
}
```

Disable with `--no-router` to use Claude's default model selection.

## Human-in-the-Loop (HITL)

Enable approval prompts for sensitive operations:

```sh
# Per-request approval (default when HITL is on)
ante --hitl-mode per-request

# Auto-approve low-risk operations
ante --hitl-mode batch-risk-threshold --risk-threshold low

# Approve everything automatically (CI/automation)
ante --hitl-mode approve-all

# Disable entirely
ante --no-hitl
```

## Status Bar

On REPL startup, Ante displays a feature summary:

```
    █████╗ ███╗   ██╗████████╗███████╗
   ██╔══██╗████╗  ██║╚══██╔══╝██╔════╝
   ███████║██╔██╗ ██║   ██║   █████╗
   ██╔══██║██║╚██╗██║   ██║   ██╔══╝
   ██║  ██║██║ ╚████║   ██║   ███████╗
   ╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   ╚══════╝
   ═══════════════════════════════════════
   v0.1.0
   model │ claude-sonnet-4
   ───────────────────────────────────────
   [✓] hooks 1  [✓] mcp 2  [✓] agents 5  [✓] memories 12
```

## REPL Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/budget` | Show token/cost usage this session |
| `/interrupt` | Interrupt the current Claude response |

## Project Structure

```
crates/
  ante/                     # CLI binary (main entry point)
    src/main.rs             # CLI parsing, mode dispatch, agent context
  agent-sdk/                # Core library
    src/
      agents/               # Sub-agent loader & broker protocol
      event.rs              # EventBus, EventPayload types
      hitl.rs               # Approval manager & risk levels
      hooks/                # Hook registry, executor, blocklist
      init.rs               # First-run setup (~/.ante/, ~/ai-wiki/)
      mcp/                  # MCP client, registry, tool discovery
      memory/               # MemoryStore, MemoryServer (embedded MCP)
      router.rs             # Model routing engine
      sessions/             # SessionManager, JSONL recording, recovery
      settings.rs           # Settings loader (JSON)
      todo.rs               # TodoList
      ui/                   # Diagram renderer, status bar
      claude.rs             # Claude CLI connection
  protocol-shape/           # Shared types (Settings, EventType, etc.)
```

## Building from Source

```sh
cargo build --release -p ante
./target/release/ante init
./target/release/ante
```

For development:

```sh
cargo build -p ante
# or with hot-reload:
cargo watch -x 'build -p ante'
```

## Credits

Forked from Anthropic's Claude Code. The extensibility layer, session system, memory server, model router, HITL, sub-agents, wiki-memory integration, and CLI restructuring are original additions.
