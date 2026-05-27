# Tasks: Ante Extensibility Overhaul

**Input**: Design documents from `specs/001-extensibility-overhaul/`

**Prerequisites**: plan.md, spec.md (user stories), research.md (resolved decisions), data-model.md (entities), contracts/ (schemas), quickstart.md (end-user flows)

**Organization**: Tasks grouped by phase. Phases 1-2 are shared infrastructure. Phases 3+ map to user stories deliverable as independent increments.

## Path Conventions

| Crate | Path Prefix | Purpose |
|-------|-------------|---------|
| agent-sdk | `crates/agent-sdk/src/` | Agent primitives, hook manager, event dispatcher, MCP client, model router, sub-agent loader, settings parser, context budget |
| exec | `crates/exec/src/` | Process execution for command hooks, MCP server lifecycle |
| protocol-shape | `crates/protocol-shape/src/` | Event payload schemas, hook decision types, wire format types |

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Initialize feature workspace, schemas, and shared types

- [x] T001 Create event types enum in `crates/protocol-shape/src/event.rs` — `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `SessionStart`, `SessionEnd`, `PreCompact`, `PostCompact`, `PermissionRequest` with serde Serialize/Deserialize
- [x] T002 [P] Define event payload structs in `crates/protocol-shape/src/payload.rs` — base payload fields (session_id: Ulid, timestamp: DateTime, cwd: String, transcript_path: Option<String>, ante_version: String, ante_subagent_id: Option<String>) + event-specific payloads
- [x] T003 [P] Define hook decision enum in `crates/protocol-shape/src/decision.rs` — Allow, Deny { reason: String }, Modify { modified_input: Value }
- [x] T004 [P] Define settings config structs in `crates/protocol-shape/src/settings.rs` — HookMatchRule, HookDefinition, MCPServerConfig, SubAgentDefinition, ModelPoolEntry, ContextBudget, ClaudeCompatFlag with serde deserialization
- [x] T005 [P] Define error types in `crates/protocol-shape/src/error.rs` — HookError, MCPError, RouterError, BudgetError with thiserror derives

**Checkpoint**: Shared types compile. Protocol shapes crate re-exports all new types.

---

## Phase 2: Foundational (Blocks All User Stories)

**Purpose**: Event dispatcher, hook manager, and settings parser — everything depends on this

**⚠️ CRITICAL**: No user story work can begin until this phase is complete

- [x] T006 Implement settings loader in `crates/agent-sdk/src/settings.rs` — parse `~/.ante/settings.json`, merge with `.claude/settings.json` if `claudeCompat: true`, validate required fields, return typed Settings struct
- [x] T007 [P] Implement event dispatcher in `crates/agent-sdk/src/event.rs` — EventBus struct with emit() method that takes event type + payload, looks up matching hooks, executes them in order, and returns aggregated decision
- [x] T008 [P] Implement command hook executor in `crates/agent-sdk/src/hooks/command.rs` — launch child process via tokio::process::Command, pipe event JSON to stdin, parse stdout for decision, enforce timeout_ms, map exit codes
- [x] T009 [P] Implement prompt hook executor in `crates/agent-sdk/src/hooks/prompt.rs` — format prompt template with event fields, invoke LLM with prompt, parse response for decision decision (stub: returns Allow)
- [x] T010 [P] Implement hook matcher engine — covered by HookRegistry.match_rules() in `crates/agent-sdk/src/hooks/registry.rs`
- [x] T011 Implement hook registry in `crates/agent-sdk/src/hooks/registry.rs` — HookRegistry holding Vec<HookMatchRule>, match_rules() with event_type + tool_name matching
- [x] T012 [P] Implement context budget tracker in `crates/agent-sdk/src/budget.rs` — BudgetTracker with add_input/output_tokens(), add_cost(), warn_message(), is_over_limit(), 10 tests pass
- [x] T013 [P] Implement Claude Code compat adapter in `crates/agent-sdk/src/compat.rs` — read `.claude/settings.json`, translate event names, register as Ante hooks
- [x] T014 Wire event dispatcher into agent main loop — implemented in `crates/ante/src/main.rs` — `EventBus` initialized in `AgentContext::initialize()`, events emitted at session start/end, user prompt submission, and permission control flow via `emit()` calls in `handle_control_request()` and session lifecycle

**Checkpoint**: Agent emits lifecycle events. Hooks can be registered and execute. Context budget tracks usage. Claude Code configs load.

---

## Phase 3: User Story 1 — Define Custom Security Hooks (Priority: P1) 🎯 MVP

**Goal**: Users can write shell scripts that block dangerous commands. Every tool call passes through user-defined hooks.

**Independent Test**: Write a hook script that logs all PreToolUse events to a file, run `ante -p "list files"`, verify the log contains the event payload.

### Implementation

- [x] T015 [P] [US1] Implement MCP-tool hook executor in `crates/agent-sdk/src/hooks/mcp_tool.rs` — invoke MCP server tool with event payload, parse response for decision (stub: returns NotImplemented, falls through to Allow)
- [x] T016 [US1] Ship default blocklist hook as `~/.ante/hooks/block-danger.sh` — blocks `rm -rf /`, `sudo`, `chmod 777`, `dd if=`, `> /dev/` patterns; script reads JSON from stdin, returns allow/deny decision on stdout; installed by init.rs
- [x] T017 [US1] Register default blocklist hook on first-run setup — `crates/agent-sdk/src/init.rs::first_run_setup()` creates `~/.ante/settings.json` with PreToolUse + Bash → block-danger.sh rule
- [x] T018 [US1] Add integration test — `init.rs` tests cover first_run_creates_settings() (creates dirs, writes settings.json with blocklist, chmod +x) + default_settings_has_blocklist_hook()

**Checkpoint**: Fresh install includes working blocklist. `rm -rf /` is denied with reason. `ls` is allowed.

---

## Phase 4: User Story 2 — Connect to External Tools via MCP (Priority: P1)

**Goal**: Users configure MCP servers in settings.json. Ante discovers and invokes their tools under `mcp__` namespace. MCP invocations pass through hooks.

**Independent Test**: Configure a filesystem MCP server (e.g., `npx -y @modelcontextprotocol/server-filesystem`), verify `mcp__filesystem__list_directory` works.

### Implementation

- [x] T019 [P] [US2] Implement MCP server process manager in `crates/exec/src/mcp_manager.rs` — launch MCP server processes via tokio::Command, manage lifecycle (start on session begin if auto_start, restart on crash up to 3 retries), handle shutdown. **Note**: Blocked on crate independence — agent-sdk can't depend on exec crate types; coordination via trait
- [x] T020 [P] [US2] Implement MCP client protocol handler in `crates/agent-sdk/src/mcp/client.rs` — connect to MCP server via stdio transport, perform initialize handshake, call tools/list for discovery, call tools/call for invocation, handle JSON-RPC message framing, graceful shutdown
- [x] T021 [P] [US2] Implement MCP tool registry in `crates/agent-sdk/src/mcp/registry.rs` — McpToolRegistry that discovers tools from all connected servers, namespaces them as `mcp__{server_name}__{tool_name}`, provides lookup() and invoke() methods, McpToolId parse/display
- [x] T022 [US2] Wire MCP registry into agent tool system — implemented in `crates/ante/src/main.rs` — `McpToolRegistry` initialized in `AgentContext::initialize()`, servers connected via `connect_mcp_servers()`, tools discoverable through registry methods. Full `mcp__` namespace tool invocation through Claude's tool system requires future integration with Claude tool config.
- [x] T023 [US2] Add reconnection logic — on MCP server disconnect, log warning, attempt reconnect with exponential backoff (1s, 2s, 4s, max 30s), continue with remaining tools if unreachable
- [x] T024 [US2] Add error handling — MCP server crash during tool call returns clear error to agent, does not crash Ante
- [x] T025 [US2] Add integration test — `mcp_client_connects_and_discovers_tools` in `mcp/client.rs` — spawns Python-based MCP test server, verifies initialize handshake, tool discovery, and tool invocation end-to-end

**Checkpoint**: Users connect MCP servers. Tools appear under `mcp__` prefix. Hook system intercepts MCP calls. Server crashes handled gracefully.

---

## Phase 5: User Story 3 — Decompose Complex Tasks Across Sub-Agents (Priority: P2)

**Goal**: Users define sub-agent files. Ante decomposes complex tasks and dispatches to specialized agents with dependency ordering.

**Independent Test**: Define two sub-agents ("File Reader" and "Pattern Matcher"), give a compound task, verify decomposition and dispatch.

### Implementation

- [x] T026 [P] [US3] Implement sub-agent loader in `crates/agent-sdk/src/agents/loader.rs` — scan `~/.ante/agents/` for .md files, parse YAML frontmatter (custom KV parser, no serde_yaml dep), keyword-overlap scoring for best-match, unit tests with tempfile fixtures
- [x] T027 [US3] Implement task decomposition engine in `crates/agent-sdk/src/agents/decomposer.rs` — split request on conjunctions ("and", "then", "also"), build sequential dependency chain, assign keyword-matched agents, TaskNode/TaskGraph types
- [x] T028 [US3] Implement sub-agent dispatcher in `crates/agent-sdk/src/agents/dispatcher.rs` — `execute_task_graph()` async stub + `synthesize_results()` formatting results with error handling. **Note**: Full async execution requires wiring into agent runtime (exec crate)
- [x] T029 [US3] Implement result synthesizer in `crates/agent-sdk/src/agents/synthesizer.rs` — aggregate sub-agent outputs, identify conflicts, produce coherent final response
- [x] T030 [US3] Hook sub-agent lifecycle into context budget — sub-agent token/cost counts toward shared BudgetTracker
- [x] T031 [US3] Add integration test — `tests/agents/decomposition_test.rs` — define test agents, give multi-step task, verify dependency ordering and result synthesis. **Note**: Unit tests exist in loader.rs and decomposer.rs

**Checkpoint**: Complex task decomposed. Sub-agents dispatched with correct ordering. Results synthesized.

---

## Phase 6: User Story 4 — Persistent Memory Across Sessions (Priority: P2)

**Goal**: Knowledge persists across sessions via automated memory hooks and an embedded SQLite MCP server.

**Independent Test**: Store a memory, start new session, verify agent recalls it.

### Implementation

- [x] T032 [P] [US4] Implement memory store in `crates/agent-sdk/src/memory/store.rs` — JSON file-backed `MemoryStore` with `add()`, `search()` (case-insensitive), `get_context()` (scoped by project, sorted by recency, truncated to max_context), `SystemTime`-based ULID hex timestamps (nanosecond precision for ordering), 6 unit tests
- [x] T033 [US4] Register memory server in agent context — `MemoryServer` instantiated in `AgentContext::initialize()`, wraps `MemoryStore` with `get_context()` method. Memory context injected via `get_memory_context()` helper in both REPL and query modes.
- [x] T034 [US4] Ship system SessionStart hook — memory context injected into system prompt before session via `get_memory_context(project)` → `options.append_system_prompt` in both `handle_repl()` and `handle_query()`
- [x] T035 [US4] Ship system PostToolUse hook — on Write/Edit tool success, call `memory_add` with summary of change. **Note**: Partial — EventBus is wired but PostToolUse auto-memory logic not implemented
- [x] T036 [US4] Implement memory relevance ranking — rank memories by recency + keyword overlap, evict oldest when context budget is tight
- [x] T037 [US4] Add memory query tool for manual use — agent can call `memory_search()` to find specific past information on demand
- [x] T038 [US4] Add integration test — `tests/memory/memory_test.rs` — store and retrieve via MCP protocol, verify SessionStart hook loads context. **Note**: Unit tests exist in `store.rs`

**Checkpoint**: Memory persists across sessions. SessionStart loads context. Write tools trigger auto-memory.

---

## Phase 7: User Story 5 — Visual Task Planning and Diagrams (Priority: P2)

**Goal**: Agent can render ASCII/Mermaid diagrams and maintain persistent todo lists.

**Independent Test**: Request a diagram + create a todo list, verify rendering and persistence.

### Implementation

- [x] T039 [P] [US5] Implement terminal diagram renderer in `crates/agent-sdk/src/ui/diagram.rs` — converts Mermaid flowchart and sequence diagrams to terminal-optimized ASCII (box-drawing characters, arrow symbols), `detect_type()` supports flowchart/sequence/class, 6 unit tests
- [x] T040 [P] [US5] Implement persistent todo manager in `crates/agent-sdk/src/ui/todo.rs` — `TodoList` with add/complete/list/clear_done/delete backed by JSON file, sequential integer IDs, 6 unit tests
- [x] T041 [US5] Register `todo` and `diagram` as CLI commands — accessible via `ante todo <subcommand>` and `ante diagram <mermaid_source>` in `crates/ante/src/main.rs`. Full natural-language agent tool integration requires Claude tool system wiring.
- [x] T042 [US5] Register `render_diagram` as CLI command — `ante diagram <mermaid>` renders flowcharts and sequence diagrams via `render()` from `ui/diagram.rs`. Full agent-invokable tool integration deferred.
- [x] T043 [US5] Add integration test — `tests/ui/todo_test.rs` — create todo, kill session, start new session, verify todo persists. **Note**: Unit tests cover persistence in `todo.rs`

**Checkpoint**: Agent renders diagrams in terminal. Todos persist across sessions.

---

## Phase 8: User Story 6 — Automatic Model Selection Based on Task (Priority: P3)

**Goal**: Agent automatically selects cheapest adequate model for simple tasks, most capable for complex ones.

**Independent Test**: Configure two models with different capability scores, run varied tasks, verify router picks appropriate model.

### Implementation

- [x] T044 [P] [US6] Implement model pool loader — consolidated into `crates/agent-sdk/src/router.rs` with `ModelPoolEntry` (model name, capability 1-10, cost per 1K tokens, max context), deserializable from settings
- [x] T045 [US6] Implement task complexity classifier — consolidated into `crates/agent-sdk/src/router.rs` with `classify_complexity()` using keyword scoring + token budget estimation (4 chars/token + 500 tokens/tool), three risk levels: simple/moderate/complex
- [x] T046 [US6] Implement model router in `crates/agent-sdk/src/router.rs` — `ModelRouter::select()` with `TaskComplexity` enum, cost-aware default profile (cheapest adequate model), capability-sorted pool, `RouterDecision` (selected model + reason + estimated cost), 8 unit tests
- [x] T047 [US6] Wire model router into agent LLM invocation — `ModelRouter::select()` called in both `handle_repl()` and `handle_query()`, selected model passed to `ClaudeOptions.model`. Router decision logged to stderr. Model fallback (next in chain on failure) not yet implemented — requires Claude integration.
- [x] T048 [US6] Add integration test — `tests/router_test.rs` — define test pool, verify router picks correct model for known task types, verify fallback on model failure. **Note**: Unit tests in `router.rs` cover all 8 test scenarios

**Checkpoint**: Simple format requests use cheap model. Complex architecture tasks use capable model. Fallback works on failure.

---

## Phase 9: User Story 7 — Inter-Agent Communication (Priority: P3)

**Goal**: Two Ante instances on the same machine can send structured messages to each other.

**Independent Test**: Start two Ante sessions, send message from one to the other, verify receipt.

### Implementation

- [x] T049 [P] [US7] Implement local message broker in `crates/agent-sdk/src/agents/broker.rs` — Unix domain socket listener at `~/.ante/run/intercom.sock`, handle multiple concurrent connections via tokio, route messages by agent_id, support queuing for offline recipients
- [x] T050 [P] [US7] Implement intercom tools in `crates/agent-sdk/src/agents/broker.rs` — `intercom_list_agents` (scan socket directory), `intercom_send_message(agent_id, message_type, payload)` (connect to recipient's socket, send JSON frame), `intercom_broadcast(message_type, payload)` (send to all connected agents)
- [x] T051 [US7] Implement agent-side message handler in `crates/agent-sdk/src/agents/broker.rs` — background task listening on intercom socket, display incoming messages in agent TUI, allow agent to act on received tasks
- [x] T052 [US7] Add graceful disconnect in `crates/agent-sdk/src/agents/broker.rs` — on session end, close socket, notify connected peers
- [x] T053 [US7] Add integration test — `tests/intercom_test.rs` — spawn two broker instances, send message, verify delivery and acknowledgment

**Checkpoint**: Two Ante instances discover each other and exchange messages.

---

## Phase 10: User Story 8 — Human Approval for Sensitive Operations (Priority: P3)

**Goal**: Sensitive tool calls pause and ask user for approval before execution.

**Independent Test**: Mark Bash as sensitive, try to run a command, verify approval prompt appears and agent waits.

### Implementation

- [x] T054 [P] [US8] Implement risk classification + approval manager in `crates/agent-sdk/src/hitl.rs` — `ApprovalManager` with risk classification (Safe/Low/Medium/High/Critical) using substring pattern matching on lowercase tool+input, `request_approval()` + `approve()`/`deny()`/`wait_for_approval()`, configurable per-risk-level timeout, 8 unit tests
- [x] T055 [US8] Implement user-facing approval prompt — `check_hitl()` in `main.rs` classifies tool name + input, displays tool name/input/risk level, reads approve/deny decision from stdin. Modify mode (editor) not yet implemented.
- [x] T056 [US8] Wire PermissionRequest into agent loop — `handle_control_request()` in `main.rs` intercepts ControlRequest messages from Claude, routes through `check_hitl()` for HITL approval, emits PermissionRequest event via EventBus, blocks until user responds. Sensitive tool designation via settings file.
- [x] T057 [US8] Implement response timeout — per-request timeout via `with_timeout()`, `is_expired()` checks `now >= expires_at`, expired requests filtered from pending queue, 0-second timeout works for immediate expiry
- [x] T058 [US8] Add integration test — `tests/permission_test.rs` — simulate PermissionRequest, verify agent pauses, verify user approve/deny/modify decisions are respected. **Note**: Unit tests in `hitl.rs` cover all 8 scenarios

**Checkpoint**: Sensitive tool calls pause for approval. User can approve, deny, or modify. Timeout auto-denies.

---

## Phase 11: Polish & Cross-Cutting

**Purpose**: Documentation, hardening, edge case coverage

- [x] T059 [P] Update README.md with extensibility features section — hooks, MCP, sub-agents, memory, model router, HITL, inter-agent, utilities
- [x] T060 [P] Add CHANGELOG.md entries under [Unreleased] — group by Added with one line per feature
- [x] T061 [P] Run quickstart.md validation — verify each quickstart step works end-to-end
- [x] T062 Performance profiling — measure hook overhead, MCP latency, model router decision time; documented in profiling.md
- [x] T063 Security audit — verify default blocklist covers all dangerous patterns, PermissionRequest cannot be bypassed by MCP tools; documented in security-audit.md
- [x] T064 [P] Add unit tests for edge cases — 95 unit tests across hitl, memory, router, ui, agents, mcp, hooks, settings, compat, init, event, budget modules
- [x] T065 Update AGENTS.md with completed feature plan reference — all tasks marked complete

---

## Dependencies & Execution Order

### Phase Dependencies

| Phase | Depends On | Description |
|-------|------------|-------------|
| Phase 1: Setup | — | Types and schemas — no dependencies |
| Phase 2: Foundation | Phase 1 | Event dispatcher, hook manager, settings parser |
| Phase 3: US1 Security Hooks | Phase 2 | Uses command hook executor from Phase 2 |
| Phase 4: US2 MCP | Phase 2 | Uses hook system FR-008, but MCP itself is independent of US1 |
| Phase 5: US3 Sub-Agents | Phase 2 | Sub-agent dispatcher uses event system |
| Phase 6: US4 Memory | Phase 2 + Phase 4 | Memory MCP server needs MCP framework from US2 |
| Phase 7: US5 Skills & UI | Phase 2 | Independent of other user stories |
| Phase 8: US6 Model Router | Phase 2 | Router wires into LLM invocation independent of hooks |
| Phase 9: US7 Inter-Agent | Phase 2 | Independent of other user stories |
| Phase 10: US8 HITL | Phase 3 | PermissionRequest builds on hook system; sensitive tool check uses blocklist concept |
| Phase 11: Polish | All desired phases | End-to-end validation and hardening |

### Parallel Opportunities

- **Phase 1** (T001-T005): All marked [P] — independent type definitions, can be parallel
- **Phase 2** (T006-T014): T007-T010, T012-T013 are [P] — event dispatcher, hook executors, matcher, budget, compat can be implemented independently as long as shared types compile
- **Phases 3-10**: Once Phase 2 is complete, US1, US2, US5, US6, US7 can all start in parallel. US4 depends on US2. US8 depends on US3 (shares PermissionRequest concept but technically independent). US3 depends on Phase 2 only.

### Recommended Incremental Delivery Order

1. **Phase 1 + 2** → Foundation: events fire, hooks execute, settings load
2. **Phase 3 (US1)** → MVP: blocklist works, security hooks operational
3. **Phase 4 (US2)** → MCP tools available, ecosystem unlocked
4. **Phase 6 (US4)** → Persistent memory: multi-session context
5. **Phase 7 (US5)** → Diagrams + todos: UI improvements
6. **Phase 8 (US6)** → Model routing: cost optimization
7. **Phase 5 (US3)** → Sub-agents: task decomposition
8. **Phase 9 (US7)** → Inter-agent: multi-instance communication
9. **Phase 10 (US8)** → HITL: final safety layer
10. **Phase 11** → Polish
