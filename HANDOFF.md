---
title: Handoff — merge ante-fork back toward upstream
date: 2026-09-06
status: ready-for-merge-probe
repo: /home/cheta/code/ante-fork
---

# Handoff: merge our fork into current upstream

## 1. Where things stand

- Single canon folder: `/home/cheta/code/ante-fork` (the old `ante-preview`, renamed).
  The duplicate `ante-spec` clone is deleted. There never was an `ante-spec` GitHub
  repo — both folders were clones of the same repo, so nothing was ever "unstaged".
- `ante-fork` is clean: HEAD `0094d51` ("docs: record missing karpathy wiki
  dependency"), in sync with `origin/main` (`aaaronmiller/ante-preview`).
- Upstream remote is configured and fetched: `upstream` =
  `https://github.com/AntigmaLabs/ante-preview`, tip `1b2138a` (v0.preview.94 sync).
- Shell note (2026-09-06): the agent's persistent-shell cwd died with the deleted
  `ante-spec` folder, so `bash` erred until restart. `eval` (JS/Bun) was used for
  all git work below. After restart, plain `bash` should work again.

## 2. Divergence, measured (not guessed)

- Merge-base of `main` and `upstream/main`: `8101e06` (upstream v0.preview.37).
  No history rewrite on either side (old merge `afcbf79`'s second parent is an
  ancestor of current `upstream/main` — the DAG is honest).
- Fork is **14 commits ahead** (our work) and **198 commits behind** (upstream
  v0.preview.37 → v0.preview.94).
- Our 14 commits: the extensibility-overhaul feature (`474fd7e`: hook system, MCP
  integration, multi-agent orchestration, persistent memory, skills/UI, dynamic
  model switching, inter-agent comms, HITL approval — 65 tasks / 11 phases per
  `specs/001-extensibility-overhaul/`), plus status-bar/scenario tests, live-model
  test suite, docs screenshots/badges. Net vs base: **171 files, +36k/−218**.
- Upstream's 198 commits are mostly release syncs, docs-site assets, CI, plus
  **structural refactors that matter**: `crates/agent-sdk` renamed to
  `crates/ante-sdk`, new `ante-llm` extraction, new `ante-acp` crate, new
  `ante-harbor/` (renamed from `run-habor-workflow.md` + `.github/ante/` scripts),
  new workflows (`harbor-adapter`, `public-crates`), `rustfmt.toml`, LICENSE.

## 3. Conflict shape (from a real trial merge, then discarded)

Method: `git worktree add /tmp/ante-merge-probe`, `git merge --no-commit
upstream/main`, inspected, then `git worktree remove --force`. Canon repo untouched
(verified clean afterwards). Result: **only 6 conflicted paths** — this merge is
feasible, re-implementation is NOT needed as plan A.

| # | Path | Type | Cause / resolution |
|---|------|------|--------------------|
| 1 | `CHANGELOG.md` | UU content | Both appended releases. Take upstream, re-add our entries on top. |
| 2 | `README.md` | UU content | Badges/screenshots vs upstream README overhaul. Hand-merge; keep both feature blurbs. |
| 3 | `crates/protocol-shape/Cargo.toml` | UU content | Dependency metadata both sides touched. Reconcile by hand (small file). |
| 4–6 | `crates/agent-sdk/{Cargo.toml, README.md, src/lib.rs}` | UD (we modified, upstream deleted) | Upstream **renamed the dir** to `crates/ante-sdk/`. Adopt the rename: port our edits onto the `crates/ante-sdk/*` counterparts, delete the old paths. |
| + ~20 notices | `crates/agent-sdk/src/agents/*`, `hooks/*`, etc. (our new files) | file-location | Git suggests the renamed destination (`crates/ante-sdk/...`) automatically. Accept its suggestion for each. |

Everything else — our `crates/ante` changes (`main.rs`, `mcp_server.rs`,
`status.rs`, 7 new test files), new `crates/snake`, `specs/`, `scenarios/`,
`research/`, `ante-test/`, `.specify/`, `.pi/` — merged **cleanly**.

## 4. Recommended course (plan A: merge, not re-implement)

1. `cd /home/cheta/code/ante-fork && git fetch upstream origin`
2. `git checkout -b merge/upstream-v0.94 && git merge upstream/main`
   (expect exactly the 6 conflicts above)
3. Resolve per the table: CHANGELOG/README/protocol-shape by hand;
   `agent-sdk` deletions by porting our hunks into `crates/ante-sdk/` and
   accepting git's rename destinations for our new files.
4. `cargo build --workspace` then the suites from the overhaul plan:
   173 unit tests across the 3 crates + 28 integration tests. All green = gate.
5. `cargo clean` (don't re-accumulate 4.8 GB of `target/`), push branch to
   `origin`, open PR against upstream (or against `origin/main` first for review).
6. Recovery: the branch is disposable — `git merge --abort` / delete branch
   returns to `0094d51` at any point before commit.

## 5. Fallback (plan B: re-implement on fresh upstream)

Only if step 3 reveals the `ante-sdk → ante-sdk` port is semantically tangled
(e.g. our hook/MCP APIs collide with upstream's new `client.rs`/`connect.rs`/
`endpoint.rs` in `ante-sdk`, or `ante-llm` duplicates our provider work). Then:
fresh `git worktree` (or clone) at `upstream/main`, re-apply the feature set
using `specs/001-extensibility-overhaul/tasks.md` as the build checklist (it is
the complete 65-task record of what was built), crate by crate. Do NOT attempt
this pre-emptively — the conflict set is too small to justify it sight unseen.

## 6. Next action for the operator

Restart the harness (reseats the dead shell cwd), then run plan A steps 1–2 and
inspect the 6 conflicts. If they match §3, finish the merge. If anything looks
materially different (different count, new UU files), stop and re-audit before
resolving — it means upstream moved again since `1b2138a`.

## 7. Outcome (2026-09-06, executed)

- Merged on branch `merge/upstream-v0.94`, committed as `951e962`, pushed to
  `origin` (aaaronmiller/ante-preview). Branch tracks
  `origin/merge/upstream-v0.94`. No PR opened yet.
- Extra fixes beyond the predicted 6 conflicts: `agent_sdk` -> `ante_sdk`
  import rename across `crates/ante` (dep-key rename changes the extern name);
  router fallback tail now returns `AllFallbacksExhausted` (was unreachable
  variant + failing spec test `fallback_exhausts_retries` — pre-existing, the
  old fork HEAD never compiled so it never ran green).
- Gates: `cargo build --workspace` clean; `cargo test --workspace` 329 passed,
  0 failed, 14 suites (2 x `ignore` are ` ```ignore ` doc snippets, not tests).
- Plan B (re-implementation) not needed.
- Watch item: merged README now describes upstream-binary features our fork
  binary lacks (offline GGUF, gateway, `ante update`). Needs a fork-features
  README pass once config adoption (§8) lands.

## 8. Config and features to adopt from upstream (surveyed, not yet built)

1. `--profile` + `curated/` (pi, plan, harbor skill) — already in tree, our
   binary has no `--profile` flag. Wire into settings loader. Small.
2. Project `.ante/settings.json` narrow-only layering — our `settings.rs` has
   no layering. Medium.
3. `catalog.json` provider layer; port our hardcoded model pool onto
   `crates/ante-llm` provider profiles. Strategic, kills Claude-only transport
   assumptions in the router.
4. Session titles (`/rename`) — evaluate against our `SessionManager` index.
5. `ante serve --sock` + `connect()` endpoint grammar — present in merged
   `ante-sdk`; expose from our binary for editor plugins.
6. `ante-acp` crate (Agent Client Protocol, e.g. Zed integration) — evaluate;
   leave out of our workspace unless adopted (upstream covers it per-crate).
7. Skip: `ante update` channels (needs release infra), offline llama.cpp
   (needs the private core).
