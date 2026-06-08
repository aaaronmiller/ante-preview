# Ante Production Agent Deployment Notes

Date: 2026-06-06
Context: local investigation from `/home/misscheta/code/ante-preview` while preparing music-library subagent work in `/home/misscheta/Music`.

Update: production-readiness fixes were applied after the initial audit.

## Executive Finding

Ante is not just a README stub. The checkout contains a functional Rust CLI crate, SDK modules, session infrastructure, hooks, memory, todo, diagram, MCP-client code, HITL types, model-router code, and subagent registry/dispatcher primitives.

The initial audit found that `ante agents run` only performed registry lookup. This has been corrected: `ante agents run` now supports an OpenCode backend, model selection, `--agent-dir`, `--cwd`, `--output`, `--dry-run`, `--read-only`, and explicit `--skip-permissions`.

## What Was Verified

- `cargo metadata --manifest-path crates/ante/Cargo.toml --no-deps` succeeds and identifies `crates/ante` as the real CLI package.
- `cargo run --manifest-path crates/ante/Cargo.toml -- --help` builds and prints the expected top-level commands.
- `cargo build -p ante` succeeds from the repo root after adding the root workspace manifest.
- `cargo build --release -p ante` is now a valid README command.
- `./crates/ante/target/debug/ante init` works against an isolated temp HOME and creates `.ante/settings.json` plus the default hook scripts.
- Direct memory commands work: a smoke-test memory was added and found by `ante memory search audit`.
- `./crates/ante/target/debug/ante query --no-memory --no-hitl --no-router ...` starts the internal MCP server and connects to Claude Code.
- The query path reached Claude Code but failed on the local external API key with `authentication_failed`; this is an environment credential issue, not proof that the Ante query path is broken.
- `./target/debug/ante agents list`, `agents match`, and `agents run` execute.
- A demo agent file under `ante-test/demo-home/.ante/agents/music-auditor.md` is selectable and executable by `agents run`.
- `ante-test/agent-run-smoke.md` was produced by a real OpenCode-backed agent run.
- `ante-test/ante-general-smoke.md` and `ante-test/ante-general-smoke.json` were produced by the seeded default agent and verify transcript plus telemetry output.
- `ante doctor` now probes parseable agents, hook executability, session directory, memory store openability, and internal MCP memory tools.
- The embedded MCP server now lists and executes `memory_add`, `memory_search`, and `memory_get_context`.
- Direct memory commands now write to `/home/misscheta/code/wiki-memory/wiki/.meta/ante-memory.db` when `/home/misscheta/code/wiki-memory` exists.

## README Accuracy

Accurate or mostly accurate:

- `ante query`, `ante init`, `ante memory`, `ante todo`, `ante sessions`, `ante agents`, and `ante diagram` are present in the CLI.
- The source tree contains modules for sessions, hooks, memory, model routing, HITL, MCP, todo, diagram rendering, and subagent definitions.
- The CLI wraps Claude Code through stream-json stdio.

Previously stale or overstated, now fixed:

- The README build commands now work from the root workspace.
- The subagent CLI now has separate `agents match` and executable `agents run` modes.
- The embedded MCP server now exposes memory tools matching the README claim.

Still limited:

- The README implies model routing dynamically selects per query. The router exists, but the configured model pool is empty by default, so it is inactive unless settings provide models.
- The main `query` and REPL path still wrap Claude Code stdio. OpenCode support is implemented for `agents run`, not for replacing the main Claude stream-json runtime.
- Existing `~/ai-wiki` is a plain directory on this machine. Ante now uses `/home/misscheta/code/wiki-memory/wiki/.meta/ante-memory.db` directly, but replacing `~/ai-wiki` with a symlink still requires an explicit backup/migration decision.

## Deployment Test Result

Production-style model-backed deployment was achieved through Ante for `agents run`.

Reason:

- `ante query` reached Claude Code but failed authentication locally; that remains an external credential issue.
- `ante agents run` now executes through OpenCode.
- `--cli-path` remains Claude-Code-protocol oriented; OpenCode is not a drop-in replacement for the main query/REPL transport.

Demo task performed:

- Added a real `music-auditor` agent definition under `ante-test/demo-home/.ante/agents/`.
- Ran `./target/debug/ante agents run --agent-dir ante-test/demo-home/.ante/agents --read-only --model deepseek-v4-flash --output ante-test/agent-run-smoke.md "audit music folders by returning exactly ANTE_AGENT_RUN_READY and no file changes"`.
- Result: OpenCode-backed agent execution completed and wrote a transcript artifact.

## Suggestions

1. Add multi-agent decomposition/concurrency to `agents run` using `agent_sdk::agents::dispatcher::execute_task_graph`.
2. Add additional backend runners: `claude-code` and `shell-command`.
3. Add proactive OpenCode model validation before launch instead of relying on targeted stderr inspection.
4. Add `--max-agents` and `--concurrency` once multi-task decomposition is wired.
5. Add richer per-worker resource telemetry: peak RSS and token estimates.
6. Add a 10-agent swarm smoke scenario that proves memory use and concurrency behavior against a read-only fixture.
7. Add CI checks that compare README command examples against `--help` and available manifests.
8. Ensure `.zshrc` sourcing is non-fatal in bash automation, or document that Ante scripts should source a dedicated env file instead.

## Bottom Line

Ante is now usable for single-agent OpenCode-backed production tasks with markdown transcripts and JSON telemetry. Full swarm behavior still needs decomposition/concurrency and a 10-agent smoke scenario.
