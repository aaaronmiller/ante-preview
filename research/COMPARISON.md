# Ante vs Pi Agent — Feature Matrix Comparison

**Generated:** 2026-05-28
**Versions:** Ante (git `0ae66ce`), Pi Agent (v0.75.3)

---

## Feature Matrix

| # | Feature | Ante | Pi Agent | Notes |
|---|---------|------|----------|-------|
| | **CORE ARCHITECTURE** | | | |
| 1 | Language | Rust | TypeScript (Node.js) | Different ecosystems |
| 2 | Protocol | Claude Code IPC | Custom agent-loop + TUI | Ante wraps Claude; Pi is standalone |
| 3 | Package model | Single binary (static) | npm global + plugin packages | Pi needs Node runtime |
| | | | | |
| | **EXTENSION/PLUGIN SYSTEMS** | | | |
| 4 | Hook/event system | ✅ Native: 4 hook types (Cmd, Prompt, MCP Tool, SubAgent) + 14 event types | ⚠️ Extension API exists but hooks module is stale (package.json exports `./hooks` but dist doesn't have it) | Ante has a working native hook system |
| 5 | Plugin loading | ❌ No dynamic plugin loader | ✅ `ExtensionRunner` + `discoverAndLoadExtensions()` from `pi-agent-core` | Pi can auto-load npm packages as plugins |
| 6 | Extension API | ❌ No extension SDK | ✅ `ExtensionAPI`, `ExtensionFactory`, `ExtensionCommandContext` | Pi has a full developer API for extensions |
| 7 | Tool registration | ✅ Static in Rust code | ✅ Dynamic via `pi.registerTool()` | Both support tool definition |
| 8 | Event bus | ✅ `EventBus` with tokio async | ✅ `EventBus` + `EventBusController` | Both have event-driven architecture |
| 9 | Session lifecycle events | ✅ 14 event types | ✅ Rich session events (`SessionBeforeCompactEvent`, `TurnStartEvent`, `ToolCallEvent`, etc.) | Pi has more finer-grained events |
| 10 | Dynamic hook rules | ✅ `reload_rules()` at runtime | ✅ Via settings.json | Both support runtime reload |
| | | | | |
| | **SUB-AGENT ORCHESTRATION** | | | |
| 11 | Sub-agent dispatch | ✅ TaskGraph with topological ordering, parallel execution | ✅ `pi-subagents` plugin (chains, parallel, TUI) | Both support sub-agents |
| 12 | Agent definitions | ✅ `.md` files with YAML frontmatter | ✅ `.md` files with YAML frontmatter | Nearly identical format |
| 13 | Agent config fields | model, description, tools, system_prompt, max_turns, etc. | model, description, tools, skills, extensions, systemPrompt, output, defaultReads, completionGuard, etc. | Pi has more per-agent config fields |
| 14 | Chain execution | ✅ Through TaskGraph | ✅ pi-subagents chain mode | Both support sequential |
| 15 | Agent scope | ✅ user/project | ✅ user/project/both | Pi has `both` scope |
| 16 | Forked context | ❌ Not directly | ✅ `defaultContext: "fork"` | Pi can fork parent context into sub-agents |
| 17 | Output modes | ❌ Not per-agent | ✅ `output` (string path), `outputMode` (inline/file-only), `reads`, `progress` | Pi has richer output options |
| 18 | Max sub-agent depth | ❌ Not configurable per agent | ✅ `maxSubagentDepth` field | Pi has recursion guard |
| 19 | Agent registry | ✅ `AgentRegistry` with loading from files | ✅ Discovery from npm packages + agent files | Both work |
| | | | | |
| | **MCP INTEGRATION** | | | |
| 20 | MCP client | ✅ Stdio transport, JSON-RPC, tool list/call | ✅ `pi-mcp-adapter` plugin (v2.8.0) | Both support MCP |
| 21 | MCP OAuth | ❌ Not implemented | ✅ Full OAuth flow in pi-mcp-adapter | Pi has authentication |
| 22 | MCP proxy mode | ❌ Not implemented | ✅ Proxy mode (pi routes MCP calls through its own registry) | Pi has richer MCP integration |
| 23 | MCP direct tools | ❌ Not implemented | ✅ Direct tool registration from MCP servers | Pi makes MCP tools appear as native tools |
| 24 | MCP consent manager | ❌ Not implemented | ✅ Consent UI for first-time tool approval | Pi has user consent flow |
| 25 | MCP status bar | ✅ Tracked in status bar | ✅ UpdateStatusBar integration | Both track in UI |
| 26 | MCP metadata cache | ❌ Not implemented | ✅ MetadataCache with persistence | Pi optimizes cold start |
| 27 | MCP server manager | ✅ Connection lifecycle | ✅ Lifecycle + reconnect + auth | Both manage connections |
| | | | | |
| | **INTER-AGENT COMMUNICATION** | | | |
| 28 | Inter-agent messaging | ✅ `Broker` with `Transport` trait | ✅ `pi-intercom` plugin (broker + spawn) | Both have messaging |
| 29 | Session discovery | ❌ | ✅ `SessionListOverlay`, `pi-intercom list` | Pi can discover other sessions |
| 30 | Message attachments | ❌ | ✅ File/snippet attachments | Pi supports rich messages |
| 31 | Sub-agent result delivery | ❌ | ✅ `SUBAGENT_RESULT_INTERCOM_EVENT` | Pi has event-based result delivery |
| | | | | |
| | **MEMORY & STATE** | | | |
| 32 | Persistent memory | ✅ SQLite-based, auto-index | ✅ `@acontext/acontext` for adaptive context | Both have memory |
| 33 | Memory server | ✅ JSON-RPC memory server | ❌ Not directly | Ante has a dedicated memory server process |
| 34 | Vector search | ✅ Auto-indexing | ✅ Via acontext | Both support search |
| 35 | Memory hooks | ✅ Pre-compact, session-end logging | ❌ Not directly | Ante ships with memory scripts |
| 36 | Session compaction | ❌ | ✅ Full compaction system with summaries | Pi has mature compaction |
| | | | | |
| | **HUMAN-IN-THE-LOOP (HITL)** | | | |
| 37 | HITL approval manager | ✅ `ApprovalManager` with risk levels | ❌ Not natively (via MCP consent UI) | Ante has built-in HITL |
| 38 | Risk levels | ✅ Low/Medium/High/Critical | ❌ | Ante has risk classification |
| 39 | Approval modes | ✅ Auto/Manual/Review | ❌ | Ante has 3 HITL modes |
| 40 | Tool permission hooks | ✅ Hooks fire before HITL check | ✅ Via EventBus | Both have permission flow |
| | | | | |
| | **BUDGET & COST TRACKING** | | | |
| 41 | Token budget | ✅ `BudgetTracker` with max tokens and max cost | ❌ Not natively | Ante has native budget tracking |
| 42 | Cost tracking | ✅ USD cost per call | ✅ Via pi-agent-suite (external) | Pi needs plugin for this |
| 43 | Context budget | ✅ `ContextBudget` with warn_at threshold | ❌ Not natively | Ante has configurable limits |
| | | | | |
| | **SKILLS SYSTEM** | | | |
| 44 | Skills loading | ❌ Not implemented (agents are similar concept) | ✅ `~/.pi/agent/skills/` with SKILL.md format, ~80+ skills | Pi has mature skills ecosystem |
| 45 | Skill auto-loading | ❌ | ✅ Auto-discovery of SKILL.md in 4 paths | Pi finds skills automatically |
| 46 | Skill frontmatter | ❌ | ✅ YAML frontmatter with inputs/outputs/metadata | Pi has structured skill definitions |
| | | | | |
| | **WEB SEARCH** | | | |
| 47 | Web search | ❌ Not implemented | ✅ `@ollama/pi-web-search` plugin, `parallel-cli` | Pi has search plugins |
| 48 | Web extraction | ❌ Not implemented | ✅ `parallel-web-extract`, `defuddle` | Pi has extraction tools |
| 49 | Deep research | ❌ Not implemented | ✅ Multi-source research skills | Pi has research pipeline |
| | | | | |
| | **AUTOMATION & LOOPS** | | | |
| 50 | Autonomous loops | ❌ | ✅ `@lnilluv/pi-ralph-loop` (repeat-until-done) | Pi has autonomous agent loop |
| 51 | Heartbeat scheduling | ❌ | ✅ `hermes-heartbeat` for sub-minute scheduling | Pi has scheduling |
| 52 | MCP output-guard | ❌ | ✅ Output guard for content filtering | Pi has output safety |
| | | | | |
| | **MODEL MANAGEMENT** | | | |
| 53 | Model router | ✅ `ModelRouter` for routing logic | ✅ `model-resolver` + `model-registry` | Both have routing |
| 54 | Model fallback | ❌ Not directly | ✅ `fallbackModels[]` in agent config | Pi has fallback support |
| 55 | Model per-agent | ✅ Via agent config | ✅ Via agent config | Both support per-agent models |
| | | | | |
| | **UI/STATUS** | | | |
| 56 | Status bar | ✅ Rich: tool calls, sub-agents, hooks, MCP, memory, todos, budget, context bar | ✅ Via pi-agent-suite footer + pi-subagents TUI | Both have status display |
| 57 | Diagram rendering | ✅ Mermaid → ASCII | ✅ `pi-mermaid` plugin | Both support diagrams |
| 58 | Todo management | ✅ `TodoList` with file-based storage | ✅ `@juicesharp/rpiv-todo` plugin | Both have todo |
| | | | | |
| | **SECURITY** | | | |
| 59 | Shell blocklist | ✅ Block-danger script | ❌ Not natively | Ante has dangerous-command blocking |
| 60 | Output guard | ❌ | ✅ Content filtering in pi-mcp-adapter | Pi can filter outputs |
| 61 | MCP security filters | ❌ | ✅ Designed with security patterns (path deny-lists, SSRF guards, destructive gates) | Pi has more security in MCP |

## Summary

### Where Ante is stronger:
1. **Native hook system** (4 types, 14 events, working implementation) — Pi's hooks module is stale/broken
2. **Built-in HITL** with risk levels and approval modes
3. **Budget/cost tracking** (token and USD budgets)
4. **Memory server** with JSON-RPC and auto-indexing
5. **Rust** — performance, safety, single-binary deployment
6. **Status bar** — richer display with more metrics

### Where Pi is stronger:
1. **Plugin ecosystem** (10+ npm packages, dynamic loading)
2. **Extension API** for third-party developers
3. **Skills system** (~80+ SKILL.md files with auto-discovery)
4. **MCP integration depth** (OAuth, proxy mode, direct tools, consent manager, metadata cache)
5. **Web search & extraction** (ollama-web-search, parallel-cli, defuddle)
6. **Autonomous loops** (ralph-loop, heartbeat scheduling)
7. **Intercom** (session discovery, message attachments, result delivery events)
8. **Per-agent configurability** (fallback models, output modes, forked context, completion guard)
9. **Session compaction** (mature, proven)
10. **Security patterns** in MCP integration (path deny-lists, SSRF guards, destructive gates)

### Observability trend (from MCP plugin research):
- **Code Mode pattern** — one execution tool replacing 100s (MySQL MCP: 1 tool replaces 192; MetaMCP: 6 tools replace N servers)
- Multi-model bridges (Ultra MCP, Zen MCP) becoming standardized
- OAuth 2.1 for MCP becoming standard
- Vector search + pricing dashboards as common add-ons
