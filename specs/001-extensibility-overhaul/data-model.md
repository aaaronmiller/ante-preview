# Data Model: Ante Extensibility Overhaul

## Entities

### Event

A lifecycle event emitted by the agent core at defined interception points.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `session_id` | ULID | yes | Unique session identifier |
| `event_type` | String | yes | One of: PreToolUse, PostToolUse, PostToolUseFailure, UserPromptSubmit, SessionStart, SessionEnd, PreCompact, PostCompact, PermissionRequest |
| `tool_name` | String | conditional | The tool being invoked (null for non-tool events) |
| `tool_input` | Object | conditional | The tool's input arguments (null for non-tool events) |
| `tool_output` | Object | conditional | The tool's result (PostToolUse/PostToolUseFailure only) |
| `cwd` | String | yes | Current working directory |
| `transcript_path` | String | yes | Path to the session transcript file |
| `ante_version` | String | yes | Version of Ante that emitted the event |
| `ante_subagent_id` | String | no | If event comes from a sub-agent |
| `timestamp` | ISO-8601 | yes | When the event was emitted |
| `error` | Object | no | Error details (PostToolUseFailure only) |

**State transitions**: Events are emitted in a linear sequence within a
session. No branching — each event represents a point in the single
agentic loop iteration.

### HookDefinition

A single hook configured to run when a matching event fires.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | Enum | yes | `command`, `prompt`, or `mcp-tool` |
| `command` | String | conditional | Shell command (type=command) |
| `prompt` | String | conditional | LLM prompt template (type=prompt) |
| `mcp_server` | String | conditional | MCP server name (type=mcp-tool) |
| `mcp_tool` | String | conditional | Tool name on the MCP server (type=mcp-tool) |
| `timeout_ms` | Integer | no | Max execution time before hook is killed (default: 30000) |
| `run_async` | Boolean | no | If true, hook runs without blocking the agent loop |
| `max_depth` | Integer | no | Max hook nesting depth to prevent cycles (default: 3) |

**Validation rules**:
- Must have exactly one of `command`/`prompt`/`mcp_server`+`mcp_tool` set
- `timeout_ms` must be between 100 and 300000
- `max_depth` must be between 1 and 10

### HookMatchRule

Maps lifecycle events to hook definitions.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `event_type` | String | yes | Which event to match |
| `matcher` | String | yes | Regex pattern against tool_name (use `".*"` for all) |
| `hooks` | [HookDefinition] | yes | Ordered list of hooks to execute |
| `mode` | Enum | no | `all` (run all matching hooks) or `first` (stop at first decision, default) |

### MCPServerConfig

Configuration for an external MCP-compliant tool server.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | yes | Logical name for the server |
| `command` | String | yes | Command to launch the server (e.g., "npx", "node") |
| `args` | [String] | yes | Arguments for the command |
| `env` | Object | no | Environment variables to set |
| `transport` | Enum | no | `stdio` (default) or `sse` |
| `url` | String | conditional | URL if transport=sse |
| `auto_start` | Boolean | no | Start server on session begin (default: true) |
| `timeout_ms` | Integer | no | Connection timeout in ms (default: 10000) |

### SubAgentDefinition

Defines a specialized sub-agent persona stored in `~/.ante/agents/`.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | yes | Unique agent name |
| `description` | String | yes | What this agent does (for task decomposition) |
| `prompt` | String | yes | System prompt for the sub-agent |
| `tools` | [String] | no | Restricted tool list (omit for full access) |
| `model` | String | no | Model override for this sub-agent |
| `max_turns` | Integer | no | Max conversation turns (default: 20) |

### MemoryEntry

A persistent knowledge record stored via the memory MCP server.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | ULID | yes | Unique memory identifier |
| `content` | String | yes | The memory content |
| `session_id` | String | yes | Session that created it |
| `project` | String | no | Project context |
| `timestamp` | ISO-8601 | yes | When created |
| `tags` | [String] | no | Categorization tags |
| `embedding` | [f32] | no | Vector embedding for semantic search |

**Validation**: Content must be non-empty and less than 100KB.

### ModelPoolEntry

A model available for the dynamic router.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | String | yes | Display name |
| `provider` | String | yes | Provider identifier (e.g., "openai", "local") |
| `model_id` | String | yes | Provider's model identifier |
| `cost_per_1k_input` | f64 | yes | Cost per 1k input tokens in USD |
| `cost_per_1k_output` | f64 | yes | Cost per 1k output tokens in USD |
| `latency_tier` | Enum | yes | `fast`, `medium`, `slow` |
| `capability_score` | Integer | yes | 1-100, higher = more capable |
| `privacy_tier` | Enum | yes | `local` (no data leaves device), `trusted` (known provider), `external` (any) |

### ContextBudget

Tracks resource usage within a session.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `max_tokens` | Integer | yes | Maximum total tokens for the session |
| `max_cost_usd` | f64 | yes | Maximum API cost for the session |
| `warn_threshold_pct` | Integer | no | Warning at this percentage (default: 80) |
| `current_tokens` | Integer | - | Runtime counter (not persisted) |
| `current_cost` | f64 | - | Runtime counter (not persisted) |

## Relationships

```
settings.json
├── hooks → { event_type → [HookMatchRule] }
│   └── HookMatchRule.hooks → [HookDefinition]
├── mcpServers → { name → MCPServerConfig }
├── agents → { name → SubAgentDefinition }
├── modelPool → [ModelPoolEntry]
└── budget → ContextBudget

Event (runtime, not persisted)
└── triggers → HookMatchRule → HookDefinition → decision

MemoryEntry (persisted via MCP server)
└── created_by → session_id
```
