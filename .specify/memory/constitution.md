<!--
  Sync Impact Report

  Version change: (none) → 1.0.0
  This is the initial constitution fill — no prior version to compare.

  Modified principles: (n/a — first fill)
  Added sections:
    - Core Principles (5 principles derived from agents constitution)
    - Technology Stack & Architecture (Ante-specific)
    - Development Workflow
    - Governance

  Removed sections: (none)
  Templates requiring updates:
    - .specify/templates/plan-template.md: ✅ Updated in this session (constitution check now valid)
    - .specify/templates/spec-template.md: ⚠ Already consistent — no changes needed
    - .specify/templates/tasks-template.md: ⚠ Already consistent — no changes needed

  Follow-up TODOs: (none)
-->

# Ante Constitution

## Core Principles

### I. Safety First (NON-NEGOTIABLE)
<!-- Derived from agents constitution Safety section -->
NEVER use `rm -rf` — use `rmdir` for empty dirs or `rm` with explicit paths.
Destructive commands (rm, delete, drop, reset, force-push) MUST show dry-run
and ask the user first. No wildcards with `rm` unless explicitly approved.
All tool calls MUST pass through security hooks before execution to prevent
prompt injection or buggy MCP servers from executing destructive actions
without user consent.

**Rationale**: Single most important trust mechanism. Prevents catastrophic
data loss from a single wrong command or compromised extension.

### II. Research Before Building (NON-NEGOTIABLE)
<!-- Derived from agents constitution Research Mandate section -->
Before starting any feature or capability, MUST research existing solutions.
Minimum research includes local workspace checks, skill registries
(skills.sh, agentskills.io), and GitHub search. Surface alternatives with
a comparison to the proposed approach. Continue searching until confident
no existing solution fits.

**Rationale**: Avoids reinventing wheels. The Ante project sits in a fast-moving
ecosystem — existing tools and libraries should be integrated, not duplicated.

### III. Synthesis Verification
<!-- Derived from agents constitution Synthesis Verification section -->
After synthesizing or summarizing complex source material, verify faithfulness
to the source. Identify the 3 most consequential claims and check each against
the source. Check for conflation — ensure no merging of distinct concepts.
On failure, document what was asserted incorrectly, what the source said,
and the error mechanism.

**Rationale**: In AI agent development, incorrect context is the root cause of
most bugs. Faithful synthesis prevents cascading errors from misread docs.

### IV. Changelog Discipline
<!-- Derived from agents constitution Changelog Mandate section -->
All projects MUST maintain CHANGELOG.md in the project root. Updates go under
[Unreleased] with clear summaries grouped under Added, Fixed, Changed, or
Removed. On release, move to versioned section with date. Keep entries
concise but descriptive — one line per change.

**Rationale**: Essential for tracking what changed between releases in a
fast-moving project. Critical for team coordination and user trust.

### V. Progressive Disclosure
<!-- Derived from agents constitution Progressive Disclosure section -->
Keep context files concise (<300 lines) — pointers not copies. Detailed
operational docs go to separate files referenced from context. Code style
enforced by linters/formatters, not rules files. Don't auto-generate
context files.

**Rationale**: LLM context is finite. Bloated context files degrade agent
performance. Let tools (linters, formatters, type checkers) enforce
mechanical rules; reserve prose for judgment and intent.

## Technology Stack & Architecture

**Language**: Rust (primary core), ~15MB binary target.
**Dependencies**: Zero runtime dependencies — single self-contained binary.
**Architecture**: Client-daemon — TUI, headless CLI, long-lived server (`ante serve`).
**Models**: Local-first with native local model inference. No API keys required
for basic operation. 12+ provider support for hosted models.
**Integrations**: Channel integrations (Slack, Discord via `ante gateway`),
MCP ecosystem gateway for external tool access, sub-agent orchestration.
**Testing**: Rust built-in test framework. Benchmark-driven performance
validation (Ante topped Terminal Bench 1.0 and 2.0 leaderboards).
**Config**: `~/.ante/settings.json` for hooks, MCP servers, model pool, and
context budget. Claude Code config compatibility via `claudeCompat` flag.

## Development Workflow

1. Specification-driven: spec (`/speckit.specify`) → plan (`/speckit.plan`) →
   tasks (`/speckit.tasks`) → implement (`/speckit.implement`).
2. AGENTS.md MUST contain current plan reference between `<!-- SPECKIT START -->`
   and `<!-- SPECKIT END -->` markers.
3. All PRs/reviews MUST verify constitution compliance.
4. Complexity MUST be justified in plan before implementation — every violation
   of the "simplest thing that works" rule requires documented rationale.
5. Performance benchmarks REQUIRED for any change that affects the hot path
   (tool execution loop, LLM invocation, context compaction).
6. Semantic versioning for all releases.
7. Change descriptions in commit messages must be specific enough to understand
   the intent without reading the diff.

## Governance

This constitution supersedes all other project practices. Amendments require
documented rationale, approval, and a migration plan.

**CONSTITUTION_VERSION** follows semver:
- MAJOR: Backward incompatible governance/principle removals or redefinitions.
- MINOR: New principle/section added or materially expanded guidance.
- PATCH: Clarifications, wording, typo fixes, non-semantic refinements.

Compliance verified at:
- PR review (manual check of constitution alignment)
- `/speckit.plan` constitution check gate (automated via `.specify` workflow)
- All PRs/reviews MUST verify compliance

**Version**: 1.0.0 | **Ratified**: 2026-05-19 | **Last Amended**: 2026-05-19
