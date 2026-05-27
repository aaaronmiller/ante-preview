# Research: Ante Extensibility Overhaul — Design Decisions

## Resolved: Memory MCP Server Choice

**Issue**: What concrete MCP server to use for persistent memory storage?

**Decision**: Implement a lightweight, embedded SQLite-based memory server
that runs as an internal MCP server (stdio transport, no network overhead).

**Rationale**:
- Aligns with Ante's local-first, zero-dependency philosophy
- SQLite compiles statically into the Rust binary via `rusqlite` — no external
  process management, no network ports, no installation steps
- Sub-millisecond query latency vs network RTT for external memory servers
- Matches the existing architecture (everything in a single binary)
- Supports vector search for semantic memory via simple embedding matching
  without needing a separate vector database

**Alternatives considered**:
- **Kronos**: Purpose-built memory MCP server, but adds an external Node.js
  dependency — conflicts with zero-dependency goal and adds 50+ MB
- **Loom**: Similar to Kronos — external Python/JS dependency
- **External DB (PostgreSQL via MCP)**: Overkill for single-user agent, adds
  infrastructure burden
- **File-based (JSON/Markdown)**: Simpler but no query capability, no semantic
  search, poor performance at scale

## Resolved: MCP Client Protocol Implementation

**Issue**: How to implement MCP protocol communication in Rust?

**Decision**: Use `rmcp` (Rust MCP) crate for the client-side protocol,
coupled with Ante's existing tokio process management for stdio transport.

**Rationale**:
- `rmcp` is the most mature Rust MCP library, built on tokio
- Supports both stdio (internal servers) and SSE/HTTP (external servers)
- Handles JSON-RPC message framing, transport, and lifecycle automatically
- Statically links — no additional runtime dependencies
- Community-maintained, MIT/Apache dual-licensed

**Alternatives considered**:
- **Custom protocol implementation**: Avoids external dependency but duplicates
  significant effort — JSON-RPC framing, transport handling, error recovery
- **`mcp-client` crate**: Less mature, narrower transport support

## Resolved: Inter-Agent Message Broker

**Issue**: What local communication mechanism for inter-agent messaging?

**Decision**: Unix domain sockets with a custom lightweight protocol (JSON
frames over tokio's `UnixListener`/`UnixStream`).

**Rationale**:
- Zero additional dependencies — Unix sockets are OS primitives
- Sub-millisecond latency (no TCP stack, no serialization overhead beyond JSON)
- Works without any infrastructure (no broker process to install/run)
- Tokio has first-class Unix socket support

**Alternatives considered**:
- **MQTT (Mosquitto)**: Adds a broker process, 5+ MB dependency, overkill for
  local-only communication
- **Redis Pub/Sub**: Adds a Redis dependency, network overhead, operationally
  complex for something that should be transparent
- **gRPC**: Protocol Buffers overhead, not worth it for local message passing
- **Named pipes / FIFO**: Too limited for bidirectional streaming

## Resolved: Event Payload Schema

**Issue**: What shape should lifecycle event payloads take?

**Decision**: Match Claude Code's event schema as closely as possible for
direct hook portability, with Ante-specific extensions under `ante_` prefix.

**Rationale**:
- FR-011 requires Claude Code config compatibility
- Matching the schema means existing Claude Code hooks work without
  modification — the adapter is just a config parser, not a schema translator
- Ante extensions (e.g., `ante_subagent_id`, `ante_worktree`) add value without
  breaking compatibility

**Base payload fields**:
```json
{
  "session_id": "ulid-string",
  "tool_name": "Bash",
  "tool_input": {"command": "ls -la"},
  "cwd": "/home/user/project",
  "transcript_path": "/tmp/ante-session-abc123.md",
  "ante_version": "0.1.0"
}
```

**Alternatives considered**:
- **Custom Ante-only schema**: Simpler but breaks Claude Code hook portability
- **Abstract event bus (e.g., eventbus crate)**: Over-engineered for the
  current scope, adds complexity without proportional benefit

## Resolved: Dynamic Model Router Strategy

**Issue**: What routing strategy for model selection?

**Decision**: Rule-based routing for v1, with an extensible interface for
future ML-based routing.

**Rationale**:
- Simple to implement, test, and debug
- Covers the 80% use case: cheap model for simple tasks, capable model for
  complex ones
- The `capability_score` and `cost` labels in the model pool config provide
  enough granularity for rule-based decisions
- Interface can be swapped for MTRouter or similar later without changing
  the rest of the system

**Rules (v1)**:
1. Task type classification via keyword heuristics (format/edit → simple,
   architecture redesign → complex)
2. Token budget estimation: estimated output tokens < 500 AND no new file
   creation → simple, else → complex
3. User override: explicit `--model` flag or `profile: "cost-save"` /
   `profile: "max-perf"` in settings

**Alternatives considered**:
- **MTRouter (ML-based)**: More accurate but requires training data,
  model download, ongoing maintenance — overkill for v1
- **Random assignment**: Zero value
- **User always chooses**: Defeats the purpose of automation

## Resolved: Hook Execution Model

**Issue**: Should hooks execute synchronously (blocking the agent loop) or
asynchronously (fire-and-forget)?

**Decision**: Synchronous for `PreToolUse` and `PermissionRequest` (must
block until decision received). Asynchronous with result logging for
`PostToolUse`, `SessionStart`, `SessionEnd`, `PostCompact`.

**Rationale**:
- `PreToolUse` must block — the tool cannot execute until the hook decides
- `PermissionRequest` must block — the user must approve before execution
- Post-event hooks are informational — no need to stall the agent for them
- Async hooks log failures but don't interrupt the agent

**Alternatives considered**:
- **All synchronous**: Simple but would make session startup slow (SessionStart
  hooks would block before the agent is usable)
- **All async**: Impossible for security hooks — the agent would execute
  commands before the hook can block them
