# Feature Specification: Ante Extensibility Overhaul

**Feature Branch**: `001-extensibility-overhaul`

**Created**: 2026-05-19

**Status**: Draft

**Input**: User description: "Add extensibility features to the Ante agent — hook system, MCP ecosystem integration, multi-agent orchestration, persistent memory, skills & UI enhancements, dynamic model switching, inter-agent communication, and human-in-the-loop approval."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Define Custom Security Hooks (Priority: P1)

A developer wants to prevent the agent from running destructive shell commands. They create a shell script that inspects every `Bash` tool invocation and blocks dangerous patterns like `rm -rf /`. The script is registered via a JSON config file.

**Why this priority**: The hook system is the foundational mechanism all other features build on. Without it, there is no way to customize, secure, or extend the agent's behavior.

**Independent Test**: Can be fully tested by writing a simple hook script that logs all `PreToolUse` events to a file, running a few agent commands, and verifying the log contains the expected event payloads.

**Acceptance Scenarios**:

1. **Given** a user has defined a `PreToolUse` command hook for the `Bash` tool, **When** the agent attempts to execute `rm -rf /tmp`, **Then** the hook receives the event payload, returns `{"decision": "deny", "reason": "Destructive command blocked."}`, and the agent displays the block reason without executing the command.
2. **Given** a user has defined a `PreToolUse` command hook for the `Bash` tool, **When** the agent attempts to execute `ls -la`, **Then** the hook receives the event payload, returns `{"decision": "allow"}`, and the command executes normally.
3. **Given** a user has defined a `PostToolUse` hook, **When** a tool call completes, **Then** the hook receives the result payload including tool output, exit code, and duration.

---

### User Story 2 - Connect to External Tools via MCP (Priority: P1)

A developer wants the agent to search the web for current information. They configure an MCP server entry pointing to a web search tool. The agent can then call search and fetch tools natively.

**Why this priority**: MCP integration is the primary mechanism for extending capabilities without bloating Ante's core. It unlocks the entire ecosystem of existing MCP servers.

**Independent Test**: Can be fully tested by configuring a simple MCP server (e.g., a file-system tool) and verifying the agent can discover and invoke its tools.

**Acceptance Scenarios**:

1. **Given** a user has configured an MCP server in settings, **When** the agent starts, **Then** it connects to the MCP server, discovers its tools, and makes them available for invocation.
2. **Given** an MCP server exposes a `search` tool, **When** the agent calls `mcp__web_search__search` with a query, **Then** the tool executes and returns results to the agent.
3. **Given** an MCP tool is invoked, **When** a `PreToolUse` hook is defined for MCP tools, **Then** the hook intercepts the invocation and can allow, deny, or modify it — same as built-in tools.

---

### User Story 3 - Decompose Complex Tasks Across Sub-Agents (Priority: P2)

A developer needs to investigate a bug across multiple services. The agent decomposes the task — one sub-agent searches logs, another queries the database, a third checks recent deployments — and synthesizes the findings.

**Why this priority**: Multi-agent orchestration moves Ante from a single-agent tool to a team manager, dramatically improving outcomes on complex, multi-step tasks.

**Independent Test**: Can be tested by defining two simple sub-agents (e.g., "File Reader" and "Pattern Matcher"), giving a compound task, and verifying the agent decomposes and dispatches appropriately.

**Acceptance Scenarios**:

1. **Given** a user has defined sub-agents in `~/.ante/agents/`, **When** the agent receives a complex request, **Then** it can decompose the task into a dependency graph of subtasks and dispatch each to a suitable sub-agent.
2. **Given** two subtasks have no dependency on each other, **When** the task decomposition engine creates the plan, **Then** those subtasks execute in parallel.
3. **Given** all subtasks complete, **When** the agent aggregates results, **Then** it synthesizes a coherent final response.

---

### User Story 4 - Persistent Memory Across Sessions (Priority: P2)

A developer tells the agent about project conventions on Monday. On Wednesday, in a new session, the agent recalls those conventions without being told again.

**Why this priority**: This addresses the "goldfish problem" where agents forget everything between sessions, reducing repetitive instructions and improving context-aware decision-making.

**Independent Test**: Can be tested by storing a memory entry, starting a new session, and verifying the agent retrieves and acts on the stored information.

**Acceptance Scenarios**:

1. **Given** a memory MCP server is configured, **When** the agent starts a new session, **Then** a `SessionStart` hook automatically retrieves relevant memories and injects them into context.
2. **Given** the agent applies a meaningful file change, **When** a `PostToolUse` hook fires on the `Write` tool, **Then** the agent creates a memory entry summarizing the change for future recall.
3. **Given** a user asks a question that references past work, **When** the agent processes the prompt, **Then** it searches the memory store and incorporates relevant past context.

---

### User Story 5 - Visual Task Planning and Diagrams (Priority: P2)

A developer asks the agent to explain the architecture of a feature. The agent renders a Mermaid diagram in the terminal, and maintains a persistent to-do list of remaining tasks.

**Why this priority**: Improves the agent's ability to communicate complex plans and manage multi-step tasks visibly, essential for user oversight of autonomous work.

**Independent Test**: Can be tested by requesting a simple architecture diagram and verifying ASCII-art rendering, or by creating a task list and checking persistence across turns.

**Acceptance Scenarios**:

1. **Given** the user requests a diagram, **When** the agent renders it, **Then** the output uses terminal-friendly ASCII/Mermaid formatting.
2. **Given** the user creates a task list with `todo add`, **When** tasks are completed, **Then** the list updates and persists across the session.

---

### User Story 6 - Automatic Model Selection Based on Task (Priority: P3)

A developer working on a large codebase wants simple formatting fixes to use a fast local model while complex architectural decisions use a hosted model.

**Why this priority**: Enables cost optimization and performance tuning, making Ante practical for both quick interactive use and deep research.

**Independent Test**: Can be tested by configuring a cheap and expensive model, then sending tasks of varying complexity and verifying the router selects the expected model.

**Acceptance Scenarios**:

1. **Given** a user has configured multiple models with capability scores, **When** the agent receives a simple request (e.g., "format this file"), **Then** the model router selects the cheapest capable model.
2. **Given** a user has configured a "maximum performance" profile, **When** the agent processes any request, **Then** the model router always selects the most capable model.

---

### User Story 7 - Inter-Agent Communication (Priority: P3)

A developer has two Ante sessions open — one for research and one for implementation. The research session finds a fix and sends a structured task to the implementation session.

**Why this priority**: Enables complex, multi-session workflows and future "agent team" topologies.

**Independent Test**: Can be tested by running two Ante instances, sending a message from one to the other, and verifying the second instance receives and can act on the message.

**Acceptance Scenarios**:

1. **Given** two Ante instances are running with the intercom MCP server, **When** one agent sends a structured message via `intercom_send_message`, **Then** the other agent receives and displays the message.
2. **Given** an agent receives a task message from another agent, **When** it processes the message, **Then** it can accept the task and report results back.

---

### User Story 8 - Human Approval for Sensitive Operations (Priority: P3)

The agent is about to run a deployment script. Before executing, it pauses and asks the user to approve, showing the exact command and its effects.

**Why this priority**: Essential for building trust and ensuring safety in autonomous operations, particularly in production environments.

**Independent Test**: Can be tested by marking the `Bash` tool as sensitive and verifying the agent pauses and waits for approval before executing any command.

**Acceptance Scenarios**:

1. **Given** a tool is designated as sensitive, **When** the agent attempts to call it, **Then** a `PermissionRequest` event fires, the agent pauses, and a clear approval prompt is shown to the user.
2. **Given** a user denies a sensitive operation, **When** the agent receives the denial, **Then** it does not execute the tool and reports the denial reason.

### Edge Cases

- What happens when an MCP server disconnects mid-session? The agent should log the error, attempt reconnection, and continue with available tools.
- How does the system handle recursive or circular hooks (e.g., a hook that triggers another event that triggers the same hook)? The hook manager must detect and break cycles with a max-depth limit.
- How does the model router handle all models being unavailable? Fall back to a configured default model or display a clear error.
- What happens when memory exceeds the context budget? Old or low-relevance memories should be evicted first, with a warning to the user.
- How does inter-agent communication handle conflicting instructions from multiple agents? The receiving agent should queue messages and process them sequentially, with user override available.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Ante MUST emit lifecycle events at the following points: `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `SessionStart`, `SessionEnd`, `PreCompact`, `PostCompact`, and `PermissionRequest`.
- **FR-002**: Ante MUST support an event payload schema that includes `session_id`, `tool_name`, `tool_input`, `cwd`, and any hook-specific context.
- **FR-003**: Ante MUST support user-defined hooks of type `command`, `prompt`, and `mcp-tool`.
- **FR-004**: A `command` hook MUST execute a shell script, pass event data as JSON via stdin, and parse stdout for a decision (`allow`, `deny` with reason, or `modify` with updated input).
- **FR-005**: Hook decisions MUST be enforced: `allow` lets the operation proceed, `deny` blocks it and displays the reason, `modify` replaces the tool input before execution.
- **FR-006**: Ante MUST support hook matching via regex patterns on `tool_name` or event type.
- **FR-007**: Ante MUST natively support discovery and invocation of tools from any MCP-compliant server, registering them under an `mcp__` namespace.
- **FR-008**: All MCP tool invocations MUST pass through the same lifecycle event system as built-in tools, ensuring hooks can intercept them.
- **FR-009**: Ante MUST ship with a default `PreToolUse` hook that blocks dangerous shell commands (`rm -rf`, `sudo`, `chmod 777`, etc.) unless the user has explicitly approved them.
- **FR-010**: Ante MUST support hook and MCP server configuration via a single JSON file (`~/.ante/settings.json`).
- **FR-011**: Ante MUST support compatibility mode for importing Claude Code hook configurations from `.claude/settings.json`.
- **FR-012**: Ante MUST enforce a configurable global context budget (token limit and estimated API cost limit) across all sub-agents and hooks, showing warnings when limits are approached.
- **FR-013**: Ante MUST support user-defined sub-agents stored as files in `~/.ante/agents/`, with each file specifying `name`, `description`, `prompt`, and optional `tools` restriction.
- **FR-014**: When the agent receives a complex request, it MUST be able to decompose the task into a dependency graph of subtasks, dispatch each to a suitable sub-agent respecting dependencies, and synthesize results.
- **FR-015**: Ante MUST support a memory MCP server that exposes `memory_add`, `memory_search`, and `memory_get_context` tools.
- **FR-016**: Ante MUST ship with system hooks for automated memory: `SessionStart` hook injects relevant context, `PostToolUse` hook on `Write`/`Edit` tools creates memory entries.
- **FR-017**: Ante MUST support a pool of user-configured models with labels for cost, latency, capability score, and privacy level.
- **FR-018**: Before each LLM call, a model router MUST select the most appropriate model from the pool based on task complexity and user preference profile.
- **FR-019**: Ante MUST support inter-agent communication via a local message broker, with tools for `intercom_list_agents`, `intercom_send_message`, and `intercom_broadcast`.
- **FR-020**: The hook system MUST include a `PermissionRequest` event that pauses agent execution for sensitive tool calls until the user explicitly approves, denies, or modifies the action.

### Key Entities *(include if feature involves data)*

- **Hook Configuration**: Defines lifecycle event matchers and associated hooks. Contains: event type, matcher regex, hook type (command/prompt/mcp-tool), and hook-specific parameters.
- **MCP Server Configuration**: Defines an external tool server connection. Contains: server name, command + args to launch, and tool discovery metadata.
- **Sub-Agent Definition**: Defines a specialized agent persona. Contains: name, description, system prompt, and allowed tool restrictions.
- **Memory Entry**: A persistent knowledge record. Contains: content, timestamp, session ID, project context, and relevance metadata.
- **Model Pool Entry**: Defines an LLM available for routing. Contains: model identifier, cost tier, latency tier, capability score, and privacy classification.
- **Context Budget**: Tracks resource usage per session. Contains: tokens consumed, estimated API cost, configurable limits, and warning thresholds.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user can write a shell script, register it as a `PreToolUse` hook for `Bash`, and verify it blocks defined dangerous commands and allows safe ones — configuration to verification in under 10 minutes for a developer familiar with the system.
- **SC-002**: A user can configure an MCP server (e.g., web search) and invoke its tools from the agent — end-to-end setup in under 5 minutes using documented steps.
- **SC-003**: A complex task that spans 3+ distinct domains can be decomposed into sub-tasks, dispatched to specialized sub-agents, and synthesized into a coherent result — with dependency ordering correctly respected.
- **SC-004**: A user can tell the agent project conventions in one session, start a new session, and the agent proactively recalls and applies those conventions without re-prompting.
- **SC-005**: The model router selects the cheapest adequate model for simple tasks and the most capable model for complex tasks with 95% accuracy against a labeled test set.
- **SC-006**: Two Ante instances on the same machine can send structured messages to each other, and the receiving agent displays and can act on the message — communication established in under 30 seconds.
- **SC-007**: A sensitive tool call triggers a `PermissionRequest` that pauses the agent, presents a clear approval prompt, and the agent respects the user's decision (approve/deny/modify) without requiring a session restart.
- **SC-008**: The context budget system prevents the agent from exceeding configured token or cost limits, showing a clear warning at 80% and blocking at 100% with a helpful message.

## Assumptions

- The user is familiar with JSON configuration files and has basic scripting ability for creating command hooks.
- MCP servers are managed externally — Ante launches and connects to them but does not install or update them.
- Sub-agent definitions are static Markdown files; dynamic sub-agent creation based on task analysis is a future enhancement.
- The memory MCP server is configured by the user; Ante ships with hooks for automated memory but not a bundled memory server implementation.
- Inter-agent communication works within the same local network; cross-network communication is a future enhancement.
- The default security blocklist is intentionally conservative — users can customize it to fit their risk tolerance.
- Claude Code hook compatibility targets the JSON configuration format and event schema, not binary compatibility with Claude Code internals.
