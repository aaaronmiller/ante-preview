# Implementation Plan: Ante Extensibility Overhaul

**Branch**: `001-extensibility-overhaul` | **Date**: 2026-05-19 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/001-extensibility-overhaul/spec.md`

## Summary

Transform Ante from a single-session AI coding agent into a secure, extensible,
and collaborative development platform. Core additions:
- **Hook System** — Event dispatcher + hook manager for lifecycle events
  (PreToolUse, PostToolUse, SessionStart/End, etc.) supporting command, prompt,
  and MCP-tool hook types
- **MCP Ecosystem Gateway** — Native discovery and invocation of MCP-compliant
  tools, all routed through the hook system for security
- **Secure by Default** — Built-in command blocklist hook, PermissionRequest
  event for sensitive operations
- **Context Budget Manager** — Token and cost limits enforced across all
  sub-agents and hooks
- **Multi-Agent Orchestration** — Task decomposition engine + sub-agent system
- **Persistent Memory** — Automated memory via MCP server + system hooks
- **Dynamic Model Switching** — Rule-based model router for cost/performance
- **Inter-Agent Communication** — Local message broker for multi-instance
- **Skills & UI** — Terminal diagram rendering, persistent todo management

## Technical Context

**Language/Version**: Rust 2024 edition (edition = "2024" in Cargo.toml)

**Primary Dependencies**:
- serde + serde_json — JSON config parsing for hooks, MCP servers, settings
- tokio — async runtime for hook execution, MCP server connections
- chrono + ulid — event timestamps and unique IDs
- thiserror + anyhow — error handling for hook failures
- new integrations needed: MCP client library, WebSocket for inter-agent

**Storage**: Filesystem-based.
- `~/.ante/settings.json` for hooks, MCP servers, model pool
- `~/.ante/agents/` for sub-agent definitions (Markdown files)
- Memory persistence delegated to an external MCP server
  [NEEDS CLARIFICATION: concrete MCP server choice for memory — e.g., a local
  SQLite-based server, Kronos, or Loom]

**Testing**: cargo test (unit + integration).
New integration test categories needed:
- Hook lifecycle tests (events fire, hooks execute, decisions enforced)
- MCP proxy tests (tool discovery, invocation, error handling)
- Security tests (blocklist, PermissionRequest behavior)
- Context budget tests (limits enforced, warnings shown)
- Contract tests for event payload schemas

**Target Platform**: Linux + macOS (alpha). Windows on roadmap.

**Project Type**: CLI + daemon + library (client-daemon architecture).
The workspace crates provide library components (agent-sdk, protocol-shape,
exec) consumed by the main binary.

**Performance Goals**:
- Hook execution overhead: <50ms per synchronous hook
- MCP tool invocation: <200ms discovery, <100ms per call (excluding server)
- Binary size increase: <5MB over current ~15MB baseline
- Context budget tracking: <1ms overhead per tool call

**Constraints**:
- Zero runtime dependencies (single self-contained binary)
- Local-first: all core features must work offline
- Hook decision latency must not degrade user-perceived agent responsiveness
- MCP server failures must not crash the agent

**Scale/Scope**: Single-user agent with multi-instance support.
~10k-15k LOC Rust codebase across 3 workspace crates. This feature adds an
estimated 3k-5k new LOC.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

### Principle I: Safety First (NON-NEGOTIABLE)
**Gate**: All tool calls MUST pass through security hooks.
**Status**: ✅ PASS — FR-009 explicitly ships a default PreToolUse hook with a
command blocklist. The hook system architecture (FR-001 through FR-006) ensures
every tool invocation, including MCP tools (FR-008), passes through the event
dispatcher and hook manager.

### Principle II: Research Before Building (NON-NEGOTIABLE)
**Gate**: Existing solutions must be evaluated before implementing.
**Status**: ✅ PASS — The spec references existing patterns (Claude Code hook
system, MCP protocol, pi-subagents, pi-intercom). Phase 0 research will
verify specific crate choices (MCP client library, message broker).

### Principle III: Synthesis Verification
**Gate**: Complex source synthesis must be verified.
**Status**: ✅ PASS — Not directly applicable to this feature. Implementation
docs must be verified against source specifications.

### Principle IV: Changelog Discipline
**Gate**: CHANGELOG.md must be maintained.
**Status**: ✅ PASS — All implementations will include changelog entries.

### Principle V: Progressive Disclosure
**Gate**: Context files <300 lines, pointers not copies.
**Status**: ✅ PASS — AGENTS.md maintains speckit pointer. No auto-generated
context. Plan and spec are separate files, not embedded in context.

### Technology Stack & Architecture
**Gate**: Rust primary, zero deps, ~15MB binary target, local-first.
**Status**: ✅ PASS — Architecture preserves existing constraints. New
features extend rather than replace existing architecture.

### Development Workflow
**Gate**: Spec → plan → tasks → implement. Compliance at all gates.
**Status**: ✅ PASS — Following this exact `.specify` workflow. Constitution
compliance documented here.

**Overall**: ✅ ALL GATES PASS — No violations to justify.

## Project Structure

### Documentation (this feature)

```text
specs/001-extensibility-overhaul/
├── plan.md              # This file
├── research.md          # Phase 0 output (to be created)
├── data-model.md        # Phase 1 output (to be created)
├── quickstart.md        # Phase 1 output (to be created)
├── contracts/           # Phase 1 output (to be created)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
# Single project with workspace crates
crates/
├── agent-sdk/           # Agent primitives (serde, tokio, MCP client)
│   └── src/
├── exec/                # Process execution (tokio process, libc)
│   └── src/
└── protocol-shape/      # Wire formats, event schemas, ULID
    └── src/
```

**Structure Decision**: Existing workspace layout preserved. The hook manager
and event dispatcher live in `agent-sdk` (they are agent primitives). Event
schemas go in `protocol-shape` (they are wire formats). Process isolation for
command hooks uses `exec`. No new crates needed — extensions slot into
existing boundaries.

## Complexity Tracking

> No violations to justify. All gates pass.
> Complexity from the hook system is inherent to the feature (event bus +
> matcher engine + executor) and justified by the security + extensibility
> requirements. No simpler alternative provides the same guarantees.
