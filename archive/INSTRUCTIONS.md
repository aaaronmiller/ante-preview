
---

## 🛡️ 10/3/1 ISO Adversarial Audit of the Master Plan

### 10 Critical Threats & Weaknesses (Adversarial Review)

1.  **Hook System Scope Creep**: Emitting all 17 event types in Phase 1 is over-ambitious. Early instability could break the entire agent loop, preventing any forward progress. The full Claude Code compatibility promise will delay the MVP.
2.  **Task Decomposition Engine Reliability**: Automated DAG creation from natural language is an unsolved, hard AI problem. A flawed decomposition will spawn sub-agents that waste tokens and produce garbage, especially with local models of limited reasoning ability.
3.  **Model Router "Intelligence" Gap**: The rule-based router (P2) will frequently misclassify task complexity, sending complex tasks to a weak model or simple tasks to an expensive one, leading to user frustration and unnecessary costs. The absence of a feedback loop for learning from misroutings will compound this.


8.  **HITL Approval UX Bottleneck**: Pausing the agent for every sensitive tool call (P3) will be unbearably slow in practice. Users will quickly disable it, rendering the safety mechanism useless. There is no plan for batching approvals or risk‑based thresholds.
9.  **Skill File Maintainability**: Defining tool schemas and state management purely in Markdown (`SKILL.md` and scripts) is fragile. If the script fails silently, the agent will be unaware of the broken state, and debugging will be painful.


### 3 Top-Priority Mitigations

1.  **Ship a Minimum Viable Hook System First**: Implement only the `PreToolUse`, `PostToolUse`, `UserPromptSubmit`, and `SessionStart` events in Phase 1. Defer the rest. This drastically reduces initial complexity and gets the core security/workflow features into users' hands faster.
2.  **Build a Context Budget Manager Before Any Multi-Agent Feature**: Introduce a global token/request budget that the Task Decomposition Engine and sub-agent system must respect. This prevents runaway costs and resource exhaustion from day one.
3.  **Implement a "MCP Tool Sandbox" Hook**: Ensure the hook system can intercept *all* tool calls, including those from MCP servers. Add a default `PreToolUse` hook that blocks high‑risk operations (like `Bash` with `rm -rf`) unless explicitly approved, closing the critical security gap.

### 1 Overarching Recommendation

**Shift from a feature‑integration marathon to an extensibility‑platform marathon.** The plan tries to simultaneously clone multiple Pi extensions *and* build a hook system *and* add memory/orchestration. Instead, focus the entire Phase 1+2 effort on making the **Hook System and MCP Adapter** so powerful that every other capability (memory, web search, to‑do lists, inter‑agent comms) can be implemented *as user‑provided hooks and MCP servers* rather than built into Ante’s core. This reduces core bloat, leverages existing ecosystems, and gives users unlimited customization from day one.

---

Now, here are the two spec-driven design files you requested, incorporating the adversarial audit's mitigations and recommendation.

### 📄 `specify.spec` (Requirements – What & Why)

```markdown
# Ante Extensibility Overhaul - Product Specification

## Overview
Transform Ante from a single-session AI coding agent into a secure, extensible, and collaborative development platform by introducing an event-driven hook system and deep MCP integration, enabling users to customize every aspect of the agent's behavior without modifying its core.

## Core Requirements

### R1: Extensible Hook System
**What:** Ante shall support user-defined hooks that intercept key lifecycle events and can allow, block, or modify the agent's actions.
**Why:** Provides the foundation for security policies, workflow automation, context injection, and integration with external services. This is the single most requested feature for trust and enterprise adoption.
**Events (MVP):**
- `PreToolUse` (before any tool is called, including MCP tools)
- `PostToolUse` (after a tool call succeeds)
- `UserPromptSubmit` (when the user sends a message)
- `SessionStart` / `SessionEnd`
- `PreCompact` (before context compaction)
**Hook Types:** command (shell script), prompt (LLM call), mcp-tool (invoke an MCP server tool).
**Behavior:** A hook may return a JSON decision: `allow`, `deny` (with reason), or `modify` (with updated tool input). Blocking a tool call must show a clear message to the user.

### R2: MCP Ecosystem Gateway
**What:** Ante shall natively support discovery and invocation of tools from any MCP-compliant server, with token-efficient proxying.
**Why:** Grants agents access to live web search, databases, APIs, and thousands of pre-built tools without bloating Ante’s core, and enables users to bring their own tools.

### R3: Secure by Default
**What:** All tool calls, including those from MCP servers, must pass through the hook system. Ante must ship with a default `PreToolUse` hook that blocks dangerous shell commands unless the user has explicitly approved them.
**Why:** Prevents prompt injection or buggy MCP servers from executing destructive actions without user consent.

### R4: Context & Budget Management
**What:** The agent must have a configurable global context budget (token limit and API cost limit) that is enforced across all sub-agents and hooks. An explicit warning must be shown when limits are approached.
**Why:** Prevents runaway token consumption and unexpected costs, which is critical for multi-step autonomous tasks.

### R5: User-Friendly Configuration
**What:** Hooks and MCP servers shall be configured via a single, well-documented JSON file (`~/.ante/settings.json`), with support for directly importing existing Claude Code hook configurations.
**Why:** Reduces friction for power users migrating from other tools and provides a single pane of glass for all extensions.
```

### 📄 `specify.plan` (Design – How)

```markdown
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
```

These files are now ready for use in a spec-driven workflow like Speckit. If you had additional criteria in that missing "fol.owing" input, please share them and I’ll re-audit and regenerate. Otherwise, this plan de-risks the project and turns Ante into a true platform. 	
