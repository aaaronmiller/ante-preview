#!/usr/bin/env python3
"""pre_compact.py — Ante hook: log memory/program usage before context compaction.

Receives a PreCompact event payload on stdin (CompactPayload fields):
  - current_tokens: current token count in context
  - budget_tokens: context budget token limit
  - current_cost_usd: current session cost
  - budget_cost_usd: cost limit (optional)

Writes a memory entry to ~/ai-wiki/.meta/ante-memory.db and logs to
~/.ante/run/pre_compact.log.

Always returns {"type":"allow"} — this hook is observational only.
"""

import json
import os
import sys
import tempfile
import time


def ante_dir() -> str:
    """Locate the Ante data directory."""
    d = os.environ.get("ANTE_DIR", "")
    if d:
        return d
    home = os.environ.get("HOME", "")
    if home:
        return os.path.join(home, ".ante")
    return os.path.expanduser("~/.ante")


def memory_db_path() -> str:
    """Locate the shared wiki-memory-backed Ante memory store."""
    explicit = os.environ.get("ANTE_MEMORY_DB", "")
    if explicit:
        return os.path.expanduser(explicit)
    ai_wiki = os.environ.get("AI_WIKI_DIR", "")
    if ai_wiki:
        return os.path.join(os.path.expanduser(ai_wiki), ".meta", "ante-memory.db")
    home = os.environ.get("HOME", "")
    if home:
        wiki_memory = os.path.join(home, "code", "wiki-memory")
        if os.path.exists(wiki_memory):
            return os.path.join(wiki_memory, "wiki", ".meta", "ante-memory.db")
        return os.path.join(home, "ai-wiki", ".meta", "ante-memory.db")
    return os.path.expanduser("~/ai-wiki/.meta/ante-memory.db")


def ulid_timestamp() -> str:
    """Nanosecond-precision hex timestamp (ULID-compatible)."""
    ns = time.time_ns()
    return format(ns, "016x")


def append_memory_entry(entry: dict) -> None:
    """Append a MemoryEntry to the ante-memory.db JSON array file."""
    db_path = memory_db_path()
    mem_dir = os.path.dirname(db_path)
    os.makedirs(mem_dir, exist_ok=True)

    entries = []
    if os.path.exists(db_path):
        try:
            with open(db_path, "r") as f:
                entries = json.load(f)
        except (json.JSONDecodeError, OSError):
            entries = []

    entries.append(entry)

    # Atomic write via temp file + rename
    fd, tmp = tempfile.mkstemp(dir=mem_dir, prefix=".ante-memory-")
    try:
        with os.fdopen(fd, "w") as f:
            json.dump(entries, f, indent=2)
        os.replace(tmp, db_path)
    except Exception:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def append_run_log(ante: str, event: dict) -> None:
    """Append a line to the pre_compact run log."""
    run_dir = os.path.join(ante, "run")
    os.makedirs(run_dir, exist_ok=True)
    log_path = os.path.join(run_dir, "pre_compact.log")
    try:
        with open(log_path, "a") as f:
            f.write(json.dumps(event) + "\n")
    except OSError:
        pass  # Best-effort logging


def main() -> None:
    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        # If we can't read the event, allow and exit
        print(json.dumps({"type": "allow"}))
        sys.exit(0)

    ante = ante_dir()

    # Extract compact-specific fields (fall back to safe defaults)
    current_tokens = event.get("current_tokens", 0)
    budget_tokens = event.get("budget_tokens", 200_000)
    current_cost = event.get("current_cost_usd", 0.0)
    budget_cost = event.get("budget_cost_usd")

    ts = ulid_timestamp()

    # Build memory entry
    budget_str = f"${budget_cost:.4f}" if budget_cost is not None else "unlimited"
    content = (
        f"[pre-compact] tokens: {current_tokens}/{budget_tokens} "
        f"({current_tokens * 100.0 / max(budget_tokens, 1):.1f}%), "
        f"cost: ${current_cost:.4f} (budget: {budget_str})"
    )

    entry = {
        "id": "mem-" + ts,
        "content": content,
        "tags": "memory,compact,snapshot",
        "project": "default",
        "timestamp": ts[:16],  # First 16 hex chars = microsecond precision
    }

    # Append to memory store and run log
    append_memory_entry(entry)
    append_run_log(ante, event)

    # Always allow
    print(json.dumps({"type": "allow"}))
    sys.exit(0)


if __name__ == "__main__":
    main()
