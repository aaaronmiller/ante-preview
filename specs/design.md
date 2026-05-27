## 🏗️ Design: Implementation Blueprint (How)

This section details the technical approach for implementing each feature, using Ante's existing extension points and proposed new modules.

### 1. The Hook System (P0)

**Design:** We will embed an **Event Dispatcher** within Ante's core agentic loop. A new **Hook Manager** module will load user-defined configurations and execute hooks in response to these events.

#### 1.1. Event Schema
The dispatcher will emit events at the following points, directly inspired by Claude Code's comprehensive lifecycle:

- **Tool Use**: `PreToolUse`, `PostToolUse`, `PostToolUseFailure`
- **User Interaction**: `UserPromptSubmit`, `PermissionRequest`, `Notification`
- **Session Lifecycle**: `SessionStart`, `SessionEnd`, `PreCompact`, `PostCompact`
- **Agent Management**: `SubagentStart`, `SubagentStop`, `TeammateIdle`, `TaskCreated`, `TaskCompleted`
- **System**: `Stop`, `Setup`, `ConfigChange`, `WorktreeCreate`, `WorktreeRemove`

#### 1.2. Hook Manager
This module will:
1.  **Load Config**: Parse hook configurations from user/settings JSON files. This will support two formats natively: Ante's own structure and a direct compatibility mode for Claude Code's `.claude/settings.json` format.
2.  **Match Events**: Listen for events from the dispatcher and filter them using the defined matchers (regex on tool names, etc.).
3.  **Execute Hooks**: Run the matched hooks based on their type:
    - **Command Hooks**: Execute a shell script, passing event data as JSON via `stdin`. The script's exit code and stdout JSON control the outcome (allow, deny, or modify).
    - **Prompt Hooks**: Send a pre-defined prompt and event context to an LLM for a context-aware decision.
    - **MCP Tool Hooks**: Invoke a tool from a connected MCP server, allowing for complex, token-free operations.
4.  **Enforce Decisions**: Based on the hook's output, the manager will allow the operation to proceed, block it and return a message to the user, or modify the tool's input before execution.

### 2. MCP Ecosystem Integration (P0)

**Design:** This is a pure configuration task, leveraging Ante's existing MCP support. We will define standard MCP server configurations in Ante's settings file.

- **`pi-mcp-adapter`**: This is already Ante's primary method for integrating external tools. We'll create a standard configuration profile for a generic, token-efficient MCP proxy. All tools will be exposed to the agent with a `mcp__` prefix.
- **`pi-web-search`**: We'll define an MCP server configuration that launches a web search tool (like a SearXNG instance or a custom Brave Search wrapper). The `command` and `args` in the config will handle the process lifecycle. The exposed search and fetch tools will become native tools for the Ante agent.

### 3. Multi-Agent Orchestration (P1)

**Design:** This will be implemented by extending Ante's built-in sub-agent system with an intelligent **Task Decomposition Engine**.

1.  **Sub-Agent Definition**: Users will continue to define specialized sub-agents as Markdown files in `~/.ante/agents/`. Each file will specify the agent's `name`, `description`, `prompt`, and optional `tools` restriction.
2.  **Task Decomposition Engine**: When the main agent encounters a complex goal, this engine will be invoked (via a prompt hook or a built-in tool). It will:
    - Decompose the high-level task into a **Directed Acyclic Graph (DAG)** of subtasks with clear dependencies.
    - Estimate the required capabilities for each subtask.
    - Dispatch each subtask to the most suitable sub-agent (or create a new one on the fly), respecting the DAG for parallel or sequential execution.
    - Aggregate the results from all sub-agents and synthesize a final response.
3.  **Agents from `pi-astro`**: We will port the 16 curated agents from the `pi-astro` package directly into `~/.ante/agents/` files, making them available for task decomposition.

### 4. Persistent Memory (P1)

**Design:** We will implement a "Memory Layer" using a dedicated MCP server and hooks to automate memory capture and recall.

1.  **Memory MCP Server**: We'll configure a dedicated MCP server (like Kronos, Loom, or a custom SQLite-based one) in Ante's settings. This server will expose tools like `memory_add`, `memory_search`, and `memory_get_context`. The agent can use these tools manually, but the system will also use them automatically.
2.  **Automated Memory via Hooks**: We'll create a set of **System Hooks** that ship with Ante:
    - `SessionStart` hook: Automatically injects relevant project context by calling `memory_search` on the MCP server.
    - `PostToolUse` hook (on `Write`/`Edit` matchers): Analyzes significant file changes and creates new memories via `memory_add`.
    - `UserPromptSubmit` hook: Checks the user's prompt for patterns that suggest they need past context and proactively retrieves it.

### 5. Skills & UI Enhancements (P2)

**Design:** These are classic use cases for Ante's Skills system.

- **`pi-mermaid`**: A skill directory `~/.ante/skills/mermaid/` with a `SKILL.md` file. The skill's `description` will explain when to use it, and the body will instruct the agent on the specific ASCII-art formatting rules for terminal rendering.
- **`rpiv-todo`**: A skill directory `.ante/skills/todo/`. The `SKILL.md` will define the `todo` tool's schema and instruct the agent on how to manage the list. A Python script in the `scripts/` directory will handle persistent state in a local JSON file.

### 6. Dynamic Model Switching (P2)

**Design:** We'll introduce a new **Model Router** module that sits between the agent's core loop and the LLM invocation layer.

1.  **Model Pool Configuration**: In `settings.json`, users will define a pool of available models, each with labels like `cost`, `latency`, `capability_score`, and `privacy`.
2.  **Router Logic**: Before each LLM call, the router will analyze the task's estimated complexity and the user's preferences (e.g., a "cost-saving" vs. "maximum performance" profile). It will then select the most appropriate model from the pool.
3.  **Implementation**: For a first iteration, this can be a simple, rule-based router (e.g., simple tasks go to a local Gemma-4 model, complex ones go to a hosted Qwen 3.6-27B). Future iterations could use a more sophisticated, learning-based approach like MTRouter.

### 7. Inter-Agent Communication Protocol (P3)

**Design:** This feature enables multiple Ante instances to talk to each other, enabling complex team workflows.

1.  **Local Message Broker**: We'll set up a simple, local-first MQTT broker (like Mosquitto) or use a Redis Pub/Sub channel as the communication backbone.
2.  **MCP Server Adapter**: An `intercom` MCP server (adapted from `pi-intercom`) will be configured in Ante. This server will connect to the local broker and expose tools like `intercom_list_agents`, `intercom_send_message`, and `intercom_broadcast`.
3.  **Workflow Example**: An Ante instance in a "research" session finds a bug. It uses the `intercom` tool to send a structured task message to another Ante instance that is configured for "code execution" in a clean sandbox. The execution agent completes the task and sends a message back.

### 8. Human-in-the-Loop Approval (P3)

**Design:** This is a specialized application of the hook system.

1.  **`PermissionRequest` Event**: The hook system will emit a `PermissionRequest` event whenever the agent wants to call a tool that has been designated as sensitive (e.g., `Bash`, `Write`, `WebFetch`).
2.  **Built-in Hooks**: Ante will ship with a default set of hooks for this event:
    - A prompt hook that evaluates the risk of the operation based on a company security policy.
    - A command hook that can pause execution and send a rich approval request (e.g., via a desktop notification) to the user.
    - The system will wait for the user's response (approve, deny, or approve once) before allowing the tool call to proceed, effectively pausing and resuming the agent's state.

---

### 🗺️ Final Implementation Roadmap

1.  **Phase 1 (Weeks 1-2): Foundational P0 Features**
    - Implement the core **Event Dispatcher** and **Hook Manager** in Ante's loop.
    - Design and implement the hook configuration file format, including Claude Code compatibility.
    - Implement support for **Command**, **Prompt**, and **MCP** hook types.
    - Define and configure the MCP servers for `pi-mcp-adapter` and `pi-web-search`.

2.  **Phase 2 (Weeks 3-5): Power Features (P1)**
    - Develop the **Task Decomposition Engine** and integrate it with the sub-agent system.
    - Port the `pi-astro` agents and build the `pi-subagents` orchestration logic.
    - Implement the **Persistent Memory** system by configuring a memory MCP server and developing the automated memory hooks.

3.  **Phase 3 (Weeks 6-8): Usability & Innovation (P2 & P3)**
    - Create the Skill files for `pi-mermaid` and `rpiv-todo`.
    - Develop the **Model Router** with a rule-based strategy.
    - Implement the **Inter-Agent Communication** protocol using an MQTT broker and an MCP adapter.
    - Extend the hook system with the `PermissionRequest` event and build the default **HITL Approval** hooks.
    - Write comprehensive documentation for the new hook system and all integrations.

To start, I can generate the initial configuration files for the hook system and MCP integrations or create the first skill drafts. Let me know which part you'd like to tackle first.