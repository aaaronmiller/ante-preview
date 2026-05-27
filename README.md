
<p align="center">
  <img src="docs-site/static/assets/ante.png" width="80" alt="Ante" />
</p>

<p align="center">
  <a href="https://discord.gg/CbAsUR434B"><img src="https://img.shields.io/badge/Discord-Join%20Us-5865F2?logo=discord&logoColor=white" /></a>
  <a href="https://docs.antigma.ai"><img src="https://img.shields.io/badge/Docs-docs.antigma.ai-orange?logo=safari&logoColor=white" /></a>
  <a href="https://twitter.com/antigma_labs"><img src="https://img.shields.io/badge/X-@antigma__labs-black?logo=x&logoColor=white" /></a>
  <a href="https://huggingface.co/Antigma"><img src="https://img.shields.io/badge/HuggingFace-Antigma-yellow?logo=huggingface&logoColor=white" /></a>
</p>

# Ante

> **⚠️ Alpha Preview**
> Ante is currently in alpha and provided as a research preview. Expect breaking changes and incomplete functionality. macOS and Linux only.

Ante is an AI-native, cloud-native, local-first agent runtime built by [Antigma Labs](https://antigma.ai). A single ~15MB Rust binary with zero runtime dependencies — designed from the ground up for security, performance, and resistance to AI-generated slop.

## Key Features

- **Lightweight agent core** — ~15MB binary, zero dependencies. Built for minimal overhead and maximum throughput.
- **Native local models** — Built-in local inference integration. No API keys, no internet, no data leaving your device.
- **Zero vendor lock-in** — Bring your own API key or local model. Switch between 12+ providers freely. No account required.
- **Client-daemon architecture** — Run as an interactive TUI, headless CLI, or long-lived server (`ante serve`).
- **Channel integrations** — Run Ante as a Slack or Discord bot with `ante gateway`.
- **Multi-agent orchestration** — Spawn sub-agents, coordinate complex tasks across independent, decentralized, or centralized architectures.
- **Extensible** — Custom skills, sub-agents, and persistent memory across sessions.
- **Benchmark proven** — Topped the Terminal Bench 1.0 and 2.0 leaderboards. Public, reproducible evals.

## Extensibility Features

### 🪝 Hook System
Custom event-driven hooks that fire before and after tool execution — block dangerous operations, log decisions, or inject context. Supports command hooks (script execution), prompt hooks (LLM-driven decisions), and MCP tool hooks.

```sh
# Hooks defined in ~/.ante/settings.json
ante -p "run a command safely"
```

### 🔌 MCP Ecosystem Integration
Full MCP (Model Context Protocol) client with stdio transport. Connect any MCP server to extend Ante's tool ecosystem dynamically.

```sh
# Configure MCP servers in ~/.config/mcp/mcp.json
{
  "mcpServers": {
    "my-server": {
      "command": "npx",
      "args": ["-y", "@my/mcp-server"]
    }
  }
}
```

### 👥 Multi-Agent Orchestration
Sub-agent system with agent registry loading from Markdown files (YAML frontmatter), task decomposition via conjunction splitting, dependency-graph dispatch, and result synthesis with conflict detection.

```sh
# Agent definitions in ~/.ante/agents/
ante -p "decompose this complex task across my agents"
```

### 🧠 Persistent Memory
JSON file-backed memory store with case-insensitive search, TF-IDF relevance ranking, project-scoped context retrieval, and automatic post-tool-use hook for learning from execution results.

### 🔀 Dynamic Model Router
Rule-based task complexity classifier that selects the cheapest adequate model for simple tasks and the most capable model for complex ones. Configurable model pool with capability scores, cost tracking, and token budget estimation.

### 🗣️ Inter-Agent Communication
Local Unix domain socket broker enabling structured message passing between Ante instances on the same machine. Supports direct messaging, broadcasting, and pub-sub topics.

### 👁️ Human-in-the-Loop Approval
Risk-based permission system that pauses sensitive operations for user approval. Configurable risk tiers (Safe/Low/Medium/High/Critical) with substring pattern matching, per-tool sensitivity, and request timeout/expiry.

### 📋 Built-in Utilities
- **Todo List** — Persistent task management with cross-session persistence
- **Diagram Renderer** — Terminal-optimized Mermaid flowchart/sequence diagram rendering using box-drawing characters

## Usage

### Modes

| Mode | Command | Description |
|------|---------|-------------|
| Interactive REPL | `ante repl` | Full interactive session with Claude, extensibility features, and status bar |
| One-shot query | `ante query "prompt"` | Single prompt, streams response with full Ante tooling |
| Initialize | `ante init` | Create `~/.ante/` directory structure, install default blocklist hook |
| Memory | `ante memory <cmd>` | Direct memory operations (add, search, list) |
| Todo | `ante todo <cmd>` | Direct todo list operations (add, list, done, clear) |
| Agents | `ante agents <cmd>` | List available sub-agents or decompose a task |
| Diagram | `ante diagram <file>` | Render a Mermaid diagram file to terminal ASCII |

### Status Bar

On startup, Ante displays a feature summary banner with connection stats:

```
    █████╗ ███╗   ██╗████████╗███████╗
   ██╔══██╗████╗  ██║╚══██╔══╝██╔════╝
   ███████║██╔██╗ ██║   ██║   █████╗
   ██╔══██║██║╚██╗██║   ██║   ██╔══╝
   ██║  ██║██║ ╚████║   ██║   ███████╗
   ╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   ╚══════╝
   ═══════════════════════════════════════
   v0.1.0
   model │ claude-sonnet-4
   ───────────────────────────────────────
   [✓] hooks 1  [✓] mcp 2  [✓] agents 5  [✓] memories 12
   ═══════════════════════════════════════
```

During a session, the status bar shows real-time metrics on the last line:

```
● claude-sonnet-4  ▏ctx ████░░░░░░ 45%  ▏$0.23  ▏14m  ▏MCP:2  ▏Mem:12  ▏Ag:5  ▏safe
```

| Field | Description |
|-------|-------------|
| `●` / `◉` | Indicator — ● normal, ◉ warning (context >50%), ◉ critical (>80%) |
| Model | Current LLM model name |
| `ctx ████░░ 45%` | Context window usage with visual progress bar |
| `$0.23` | Cumulative session cost estimate |
| `14m` | Session elapsed time |
| `MCP:2` | Connected MCP servers |
| `Mem:12` | Stored memory entries |
| `Ag:5` | Loaded sub-agents |
| `safe` | HITL risk level (safe/low/medium/high/critical) |

### One-Shot Query

```sh
# Basic query
ante query "find and fix the failing test"

# With model override
ante query --model claude-sonnet-4 "refactor the database module"

# Disable features
ante query --no-memory --no-hitl "run this command"
```

### Interactive REPL

```sh
# Start interactive session
ante repl

# With specific CLI binary
ante repl --cli-path /path/to/claude

# With model override
ante repl --model claude-sonnet-4
```

### REPL Commands

| Command | Description |
|---------|-------------|
| `/help` | Show available commands |
| `/budget` | Show token/cost usage this session |
| `/interrupt` | Interrupt the current Claude response |
| `/model <name>` | Switch to a different model |
| `/info` | Show session info |
| `/quit` | End session |

### Configuration

Ante loads settings from `~/.ante/settings.json`. Create it with default values:

```sh
ante init
```

Key configuration sections:

| Section | Description |
|---------|-------------|
| `hooks` | Pre/post tool execution hooks with rules, matchers, and definitions |
| `mcpServers` | MCP server registry (command, args, auto-start, lifecycle) |
| `agents` | Sub-agent directory and discovery settings |
| `memory` | Memory store path, context limits, auto-indexing |
| `modelPool` | Model entries for dynamic routing (capability score, cost, context) |
| `contextBudget` | Token and cost limits with warning thresholds |
| `claudeCompat` | Claude Code settings compatibility flags |
| `sensitiveTools` | Tool patterns requiring HITL approval |

### Claude Code Hook Compatibility

If you already have Claude Code hooks in `.claude/settings.json`, Ante can merge them:

```json
{
  "claudeCompat": {
    "mergeClaudeSettings": true,
    "translateEventNames": true
  }
}
```

### Building from Source

```sh
# Prerequisites: Rust toolchain (edition 2024)

# Build the ante binary
cd crates/ante
cargo build --release

# Build all crates
cargo build --release  # from any crate directory
```

The project has 4 crates:
- `crates/ante/` — Application binary (integration layer)
- `crates/agent-sdk/` — Agent primitives, hooks, MCP client, model router, memory, HITL, broker
- `crates/exec/` — Process execution, MCP process manager
- `crates/protocol-shape/` — Event payload schemas, hook decision types, settings

## Performance
**We care about the harness not the model nor the prompts.**

Ante is designed for the **cellular-native** thesis: agents lightweight enough to run hundreds of replicas in parallel on a single machine. Its ~15MB Rust core uses a fraction of the memory, CPU, and disk I/O of comparable agents — making mass parallelism practical without specialized infrastructure.

Docker resource usage across 20 parallel tasks (Ante vs Claude Code vs Opencode):

![Resource Usage Comparison](docs-site/docs/benchmarks/compare_animated.gif)

Across 20 parallel tasks, Ante uses **~7× less peak memory**, **~9× less average CPU**, and generates **~5× less total disk I/O** than Claude Code — while completing the same workload. See the [comparison table](docs-site/docs/benchmarks/compare_table.md) and the [benchmark details](docs-site/docs/benchmarks/eval.mdx) for the evaluation methodology and results.

## Quick Start

### Installation

Ante is distributed as a single, self-contained binary with no external dependencies — just download and run.

```sh
curl -fsSL https://ante.run/install.sh | bash

# Install a specific release channel
curl -fsSL https://ante.run/install.sh | bash -s -- nightly
```

### Interactive TUI

```sh
ante
```

### Headless Mode

```sh
# Fix a bug
ante -p "find and fix the failing test in src/auth"

# Review a diff
git diff | ante -p "review this for security issues"

# Use a different provider
ante --provider openai --model gpt-5.4 -p "refactor the database module"

# Resume a saved session
ante --resume ses_01ARZ3NDEKTSV4RRFFQ69G5FAV -p "now add tests"

# Run fully offline with a local GGUF model
ante --offline-model ~/.ante/models/Qwen3.5-9B-Q4_K_M.gguf \
  -p "add error handling to src/main.rs"
```

### Server Mode

```sh
ante serve
```

### Gateway Mode

```sh
ante gateway
```

### Update Ante

```sh
ante update

# One-off update from a different channel
ante update --channel nightly
```

## Example Usages with TUI

<table>
<tr>
<td width="50%">

**[Providing Context: Files & Folders](https://docs.antigma.ai/cookbook/providing-context)**

![Adding file context with @ mentions](docs-site/static/assets/cookbook/files.gif)

</td>
<td width="50%">

**[Interrupting & Steering](https://docs.antigma.ai/cookbook/steering)**

![Interrupting the agent with Escape](docs-site/static/assets/cookbook/interrupt.gif)

</td>
</tr>
<tr>
<td width="50%">

**[Models, Providers & Thinking](https://docs.antigma.ai/cookbook/models-and-thinking)**

![Selecting a model and provider](docs-site/static/assets/cookbook/model.gif)

</td>
<td width="50%">

**[Subscription Login](https://docs.antigma.ai/cookbook/login)**

![Connecting to a provider via /connect](docs-site/static/assets/cookbook/connect.gif)

</td>
</tr>
</table>

[See all cookbook guides](https://docs.antigma.ai/category/tui-cookbook)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Clients                             │
│                                                             │
│   ┌───────────┐    ┌───────────┐    ┌────────────────────┐  │
│   │    TUI    │    │ Headless  │    │    ante serve      │  │
│   │  (ante)   │    │ (ante -p) │    │  (stdio / ws)      │  │
│   └─────┬─────┘    └─────┬─────┘    └─────────┬──────────┘  │
└─────────┼────────────────┼─────────────────────┼────────────┘
          │     Op         │                     │
          ▼                ▼                     ▼
┌─────────────────────────────────────────────────────────────┐
│                         Daemon                              │
│                                                             │
│   Session ──▶ Turn ──▶ Step                                 │
│                                                             │
│   ┌──────────┐  ┌──────────────┐  ┌───────────────────┐     │
│   │  Tools   │  │  Permission  │  │  Skills / Agents  │     │
│   └──────────┘  └──────────────┘  └───────────────────┘     │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                     LLM Providers                           │
│                                                             │
│   Anthropic · OpenAI · Gemini · Grok · Open Router · Local  │
└─────────────────────────────────────────────────────────────┘
```

## Supported Providers

Ante works with 12+ providers out of the box:

| Provider | Example Models |
|----------|---------------|
| Anthropic | Claude Sonnet 4.5, Opus 4.6 |
| OpenAI | GPT-5 family |
| Google Gemini | Gemini 3 family |
| Grok (xAI) | Grok 4 |
| Open Router | Multiple providers |
| Local (GGUF) | Any GGUF model via built-in llama.cpp |
| ...and more | Vertex AI, Zai, Antix, OpenAI-compatible |

Configure providers via environment variables (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.) or OAuth. Add custom providers in `~/.ante/catalog.json`.

## FAQ
### Why <span style="color: #f59e0b;">An</span>other <span style="color: #f59e0b;">Te</span>rminal agent
Ante is fast, lightweight, and the only terminal agent with native local inference support built in.
We believe this self-contained agent core that self organize is the centre of the future of agent economy.

It is just built different. 

<details>
<summary><b>How is Ante different than other agents</b></summary>
On the high level, it has most of your favorite features (Multi-agents, skills, etc.) of your favorite agents (like Claude Code, Codex, etc.) 

- Ante is built from scratch in native Rust, we are obsessed with being self contained, so only essential libraries without framework or runtime dependencies. 

- You only need a llm provider configured to run it. Actually if you have the hardware, you don't even need a llm provider because Ante natively support private inference engine. 

- This resulted in ~15MB self-contained binary and multi-agent orchestration designed to run hundreds of replicas in parallel at scale.
See the [benchmark details](docs-site/docs/benchmarks/eval.mdx) across 20 parallel tasks for concrete numbers.

- No vendor lock-ins, not even ourself. You don't need an account and can reuse your favorite api credentials. 

</details>

<details>
<summary><b>Why care about runtime optimization like memory and I/O if model inference is usually the biggest bottleneck?</b></summary>

For one-on-one agent interactions, runtime overhead like memory usage and I/O is often less important than model inference.

But our vision is much bigger: millions of agents self-organizing and communicating at massive scale. At that point, even small inefficiencies get multiplied millions or billions of times, so runtime optimization becomes economically significant.
</details>

<details>
<summary><b>Can I run Ante completely offline?</b></summary>

Yes. Ante has a built-in llama.cpp engine that runs GGUF models locally. It handles engine installation, model discovery, and memory management automatically. No API keys or internet connection required.
</details>

<details>
<summary><b>Can I use my own custom models or providers?</b></summary>

Yes. Create a `~/.ante/catalog.json` file to add or override providers and models with custom endpoints, API keys, and configurations. Any OpenAI-compatible API works.
</details>

<details>
<summary><b>What is the <code>ante serve</code> mode for?</b></summary>

Server mode runs Ante as a long-lived daemon that communicates over a structured JSONL protocol. It's ideal for building editor plugins, web UIs, and custom integrations on top of Ante.
</details>

<details>
<summary><b>How do I configure Ante?</b></summary>

Settings live in `~/.ante/settings.json`. You can set your default model, provider, theme, and permission policy. CLI flags override settings for individual sessions. See the [configuration docs](https://docs.antigma.ai/configuration/preference) for details.
</details>

<details>
<summary><b>Can I extend Ante with custom skills or sub-agents?</b></summary>

Yes. Drop skill files in `~/.ante/skills/` (user-level) or `.ante/skills/` (project-level) using the Open Agent Skills format. Custom sub-agents go in `~/.ante/agents/` with their own prompts, tool sets, and model overrides.
</details>

## Documentation

Full documentation is available at [docs.antigma.ai](https://docs.antigma.ai).
The source code is in `docs-site/docs`
