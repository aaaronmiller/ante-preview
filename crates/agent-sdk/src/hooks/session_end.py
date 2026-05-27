#!/usr/bin/env python3
"""session_end.py — Ante hook: log session summary at end of session.

Receives a SessionEnd event payload on stdin:
  - total_input_tokens: total input tokens used in this session
  - total_output_tokens: total output tokens used
  - total_cost_usd: approximate cost in USD
  - duration_secs: session duration in seconds
  - reason: reason for session end (optional)

Writes a memory entry to ~/.ante/memory/ante-memory.db and logs to
~/.ante/run/session_end.log.

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


def ulid_timestamp() -> str:
    """Nanosecond-precision hex timestamp (ULID-compatible)."""
    ns = time.time_ns()
    return format(ns, "016x")


def append_memory_entry(ante: str, entry: dict) -> None:
    """Append a MemoryEntry to the ante-memory.db JSON array file."""
    mem_dir = os.path.join(ante, "memory")
    os.makedirs(mem_dir, exist_ok=True)
    db_path = os.path.join(mem_dir, "ante-memory.db")

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
    """Append a line to the session_end run log."""
    run_dir = os.path.join(ante, "run")
    os.makedirs(run_dir, exist_ok=True)
    log_path = os.path.join(run_dir, "session_end.log")
    try:
        with open(log_path, "a") as f:
            f.write(json.dumps(event) + "\n")
    except OSError:
        pass  # Best-effort logging


def format_duration(secs: int) -> str:
    """Format seconds as a human-readable duration string."""
    hours = secs // 3600
    minutes = (secs % 3600) // 60
    seconds = secs % 60
    parts = []
    if hours > 0:
        parts.append(f"{hours}h")
    if minutes > 0:
        parts.append(f"{minutes}m")
    parts.append(f"{seconds}s")
    return "".join(parts)


def main() -> None:
    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError as e:
        # If we can't read the event, allow and exit
        print(json.dumps({"type": "allow"}))
        sys.exit(0)

    ante = ante_dir()

    # Extract session-end fields
    total_input = event.get("total_input_tokens", 0)
    total_output = event.get("total_output_tokens", 0)
    total_cost = event.get("total_cost_usd", 0.0)
    duration = event.get("duration_secs", 0)
    reason = event.get("reason", "unknown")

    ts = ulid_timestamp()
    total_tokens = total_input + total_output

    # Build memory entry
    content = (
        f"[session-end] session complete: "
        f"{total_tokens} tokens ({total_input} in / {total_output} out), "
        f"${total_cost:.4f}, duration: {format_duration(duration)}, "
        f"reason: {reason}"
    )

    entry = {
        "id": "mem-" + ts,
        "content": content,
        "tags": "memory,session,usage",
        "project": "default",
        "timestamp": ts[:16],
    }

    # Append to memory store and run log
    append_memory_entry(ante, entry)
    append_run_log(ante, event)

    # Always allow
    print(json.dumps({"type": "allow"}))
    sys.exit(0)


if __name__ == "__main__":
    main()
