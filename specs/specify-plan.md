# Ante Extensibility Overhaul - Technical Design Plan

## Architecture Overview
We will introduce a **Hook Manager** as a new internal module that sits between Ante's agentic loop and all tool execution. The Hook Manager loads configurations, listens for lifecycle events via an **Event Bus**, and dispatches matched hooks. MCP integration will be provided by an existing adapter, but all MCP tool invocations will be routed through the same Event Bus to ensure hooks can intercept them.

## Component Breakdown

### 1. Event Bus & Lifecycle Instrumentation
- Instrument the agent loop to emit defined event payloads at the specified lifecycle points.
- Event payloads include: `session_id`, `tool_name`, `tool_input`, `cwd`, `transcript_path`, and any hook-specific context.
- For MCP tools, the `tool_name` will follow the `mcp__<server>__<tool>` convention.

### 2. Hook Manager
- **Config Loader**: Reads `hooks` array from `~/.ante/settings.json`. Supports a `"claudeCompat": true` flag to parse `.claude/settings.json` format seamlessly.
- **Matcher Engine**: Each hook definition includes a `matcher` (regex on `tool_name` or event type) and a list of hooks to run.
- **Executor**: Runs hook based on `type`:
  - `command`: Spawns a shell subprocess, pipes event JSON to stdin, reads JSON from stdout. Exit code 0 = allow, 2 = deny.
  - `prompt`: Sends the event plus a user-defined prompt to an LLM (the current session's model or a cheap fast one), parses the response for a decision.
  - `mcp-tool`: Calls an MCP tool from a specified server, passing the event as an argument. Returns the tool's result as the decision.
- **Sandboxing**: The default configuration includes a high-severity command blocklist (`rm -rf`, `sudo`, `chmod 777`, etc.) applied via a built-in `command` hook.

### 3. MCP Integration
- Ante's existing MCP support will be extended with an **MCP Proxy** that registers all tools under the `mcp__` namespace.
- The proxy will pass every tool invocation request through the Event Bus, ensuring hooks see MCP tools the same as built-in tools.

### 4. Context Budget Manager
- A global counter tracks tokens used and estimated API cost per session.
- Before dispatching a sub-agent or a long hook, the system checks if remaining budget is sufficient. If not, it returns an error instead of starting the operation.

### 5. Configuration Schema
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "~/.ante/hooks/validate-bash.sh" }
        ]
      }
    ]
  },
  "mcpServers": {
    "web-search": { "command": "npx", "args": ["-y", "@ollama/pi-web-search"] }
  }
}
```

## Data Flow (Example: Secure Bash Execution)
1. Agent decides to run `rm -rf /tmp/build`.
2. `PreToolUse` event emitted with tool_name `Bash` and input `rm -rf /tmp/build`.
3. Hook Manager finds a matching command hook.
4. Hook runs `validate-bash.sh`, receives event JSON.
5. Script detects dangerous pattern, outputs `{"decision": "deny", "reason": "Destructive command blocked."}` and exits with code 2.
6. Hook Manager blocks the tool call and displays the reason to the user.

## Implementation Phases (Aligned with Mitigated Plan)
1. **Week 1-3**: Implement Event Bus, Hook Manager with `command` hooks, default security hook, MCP proxy integration.
2. **Week 4**: Add `prompt` and `mcp-tool` hook types, Claude Code config compatibility.
3. **Week 5**: Context Budget Manager and extensive test coverage.
4. **Week 6+**: User documentation, community hook templates, and optional additional event types (`PostToolUse`, etc.).

## Non-Goals (for MVP)
- Task Decomposition Engine, Multi-Agent Orchestration, Persistent Memory, Dynamic Model Switching, Inter-Agent Communication, HITL Approval beyond the default blocklist. These will be built by the community using the hook system and MCP servers, or added in later core releases.