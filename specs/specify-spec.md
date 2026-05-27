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