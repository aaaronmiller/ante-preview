# Status Bar + Sub-Agent Integration Test

Exercises the Ante agent CLI and sub-agent system end-to-end.

## What it tests

| # | Test | What it validates |
|---|------|-------------------|
| 1 | Binary smoke | `ante --help` exits 0 |
| 2 | First-run init | `ante init` creates `~/.ante/` skeleton |
| 3 | Agent loading | `ante agents list` shows installed agents |
| 4 | Agent matching | `ante agents run "review rust code"` picks `code-reviewer` |
| 5 | Agent matching (2) | `ante agents run "break down requirements"` picks `task-writer` |
| 6 | Memory operations | `ante memory add` + `ante memory search` roundtrip |
| 7 | Todo operations | `ante todo add` + `ante todo list` + `ante todo done` |
| 8 | Diagram render | `ante diagram` produces ASCII output |
| 9 | Status bar unit tests | `cargo test status` — 22 tests pass |

## Running

```bash
./tasks/run-status-bar-test.sh
```

## Agent definitions

- `agents/code-reviewer.md` — Rust code review specialist
- `agents/task-writer.md` — Requirement decomposition specialist

## Notes

- The `ante agents run` command currently finds and displays the best-matching
  agent but does **not** execute it. Full sub-agent dispatch through the hook
  system (`HookDefinition::SubAgent`) is wired in the event bus and dispatcher
  but requires a running Claude session to invoke.
- Sub-agent parallel execution is tested at the unit level via
  `crates/agent-sdk/src/agents/dispatcher.rs` tests.
