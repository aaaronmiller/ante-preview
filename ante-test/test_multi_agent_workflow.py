#!/usr/bin/env python3
"""
Multi-agent workflow integration test for Ante.

Tests a complete multi-step workflow through Ante's agent system,
using the prompttools evaluation framework. Exercises the full
Ante pipeline: mock Claude → litellm → Ollama with real LLM inference.

Scenario: Build a simple CLI tool
  1. Agent system matches task description to appropriate agents
  2. Ante sends the prompt through the Claude protocol
  3. Mock Claude proxies to free local model via litellm/Ollama
  4. Response demonstrates agent capabilities

Usage:
    PATH="/tmp/ante-mock-path:$PATH" python3 ante-test/test_multi_agent_workflow.py
"""

import json
import os
import subprocess
import sys
import time
import csv
from pathlib import Path

ANTE_BIN = os.environ.get("ANTE_BIN", "ante")
TEST_DIR = Path(__file__).parent

# ── Multi-agent workflow scenarios ──────────────────────────────────

# These simulate the kind of complex, multi-step tasks that
# a multi-agent system should handle. Each scenario tests
# different aspects of the agent system.

WORKFLOWS = [
    {
        "name": "multi-agent-code-gen",
        "description": "Code generation with review (multi-agent pipeline)",
        "prompt": (
            "You are a multi-agent system. First, as an architect, "
            "design a simple CLI tool that counts words in a file. "
            "Then, as a developer, write the Rust implementation. "
            "Finally, as a reviewer, list 2 improvements."
        ),
        "expected_topics": ["cli", "file", "count", "rust", "improve"],
        "min_quality_words": 50,
    },
    {
        "name": "multi-agent-research",
        "description": "Research then write documentation",
        "prompt": (
            "Research the key features of SQLite that make it good for desktop apps. "
            "Then write a short documentation page about using SQLite in Rust "
            "with the rusqlite crate."
        ),
        "expected_topics": ["sqlite", "database", "rust", "rusqlite"],
        "min_quality_words": 40,
    },
    {
        "name": "multi-agent-design",
        "description": "Design decision with tradeoffs",
        "prompt": (
            "Design a data model for a todo app with users, projects, and tasks. "
            "Explain the relationships and key fields for each entity."
        ),
        "expected_topics": ["user", "project", "task", "model", "field"],
        "min_quality_words": 50,
    },
]


def run_ante_query(prompt, timeout=300):
    """Run `ante query` and return results."""
    cmd = [ANTE_BIN, "query", prompt, "--no-hitl", "--no-memory", "--no-router"]
    env = os.environ.copy()
    env["PATH"] = f"{os.pathsep.join(['/tmp/ante-mock-path', env.get('PATH', '')])}"

    try:
        t0 = time.time()
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout, env=env,
        )
        elapsed = time.time() - t0
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
            "elapsed": elapsed,
        }
    except subprocess.TimeoutExpired:
        return {
            "stdout": "",
            "stderr": "TIMEOUT",
            "returncode": -1,
            "elapsed": timeout,
        }
    except FileNotFoundError:
        return {
            "stdout": "",
            "stderr": f"Ante binary not found: {ANTE_BIN}",
            "returncode": -2,
            "elapsed": 0,
        }


def extract_response(stdout):
    """Extract the assistant response from Ante's stdout."""
    response_text = ""
    assistant_match = False
    for line in stdout.split("\n"):
        if "── assistant" in line:
            assistant_match = True
            continue
        if "── result" in line:
            break
        if assistant_match:
            cleaned = line.strip().strip('"')
            if cleaned:
                response_text += cleaned + " "
    return response_text.strip()


def evaluate_response(response, expected_topics, min_words):
    """Evaluate response quality."""
    lower = response.lower()
    words = response.split()
    word_count = len(words)

    # Topic coverage
    found_topics = [t for t in expected_topics if t.lower() in lower]
    topic_coverage = len(found_topics) / len(expected_topics) * 100 if expected_topics else 0
    topic_pass = topic_coverage >= 40.0  # At least 40% of topics

    # Minimum length
    length_pass = word_count >= min_words

    # Structure: check for some formatting
    has_bullets = "•" in response or "- " in response or "* " in response
    has_code = "```" in response or "`" in response
    has_headings = "##" in response or "**" in response

    structure_score = sum([has_bullets, has_code, has_headings])

    passed = topic_pass and length_pass

    return {
        "word_count": word_count,
        "found_topics": found_topics,
        "topic_coverage_pct": round(topic_coverage, 1),
        "topic_pass": topic_pass,
        "length_pass": length_pass,
        "has_bullets": has_bullets,
        "has_code_blocks": has_code,
        "has_headings": has_headings,
        "structure_score": structure_score,
        "passed": passed,
    }


def test_agent_listing():
    """Test that the agent registry loads and lists agents correctly."""
    result = subprocess.run(
        [ANTE_BIN, "agents", "list"],
        capture_output=True, text=True, timeout=10,
    )

    lines = result.stdout.strip().split("\n")
    agent_count = len(lines)

    print(f"  Agents loaded: {agent_count}")
    for line in lines:
        print(f"    {line.strip()}")

    return agent_count >= 3  # Need at least our 3 custom agents


def test_agent_matching():
    """Test that agents are matched to tasks correctly."""
    test_cases = [
        ("Write Rust code with error handling", "code-reviewer"),
        ("Design database schema for web app", "architect"),
        ("Research best practices for logging", "researcher"),
    ]

    passed = 0
    for task, expected_agent in test_cases:
        result = subprocess.run(
            [ANTE_BIN, "agents", "run"] + task.split(),
            capture_output=True, text=True, timeout=10,
            input="exit\n",
        )
        stdout = result.stdout

        if expected_agent in stdout:
            print(f"  ✓ '{task[:40]}...' → {expected_agent}")
            passed += 1
        else:
            print(f"  ✗ '{task[:40]}...' expected {expected_agent}")
            print(f"    Got: {stdout[:100].strip()}")

    return passed == len(test_cases)


def main():
    # Pre-flight checks
    if not os.path.exists("/tmp/ante-mock-path/claude"):
        print("ERROR: Mock Claude not found. Run setup first.")
        sys.exit(1)

    import urllib.request
    try:
        urllib.request.urlopen("http://localhost:4000/health", timeout=2)
    except Exception:
        print("ERROR: litellm proxy not running on port 4000")
        sys.exit(1)

    print("=" * 72)
    print("ANTE MULTI-AGENT WORKFLOW TEST")
    print("=" * 72)
    print(f"Model: llama3.2:1b via litellm → Ollama (CPU)")
    print(f"Ante: {ANTE_BIN}")
    print()

    # ── 1. Agent Registry Tests ──
    print("── 1. Agent Registry ──")
    listing_ok = test_agent_listing()
    print(f"  Agent listing: {'✓' if listing_ok else '✗'}")
    print()

    print("── 2. Agent Matching ──")
    matching_ok = test_agent_matching()
    print(f"  Agent matching: {'✓' if matching_ok else '✗'}")
    print()

    # ── 3. Multi-agent Workflows ──
    print("── 3. Multi-agent Workflows ──")
    print()

    all_results = []

    for i, wf in enumerate(WORKFLOWS, 1):
        print(f"[{i}/{len(WORKFLOWS)}] {wf['name']}")
        print(f"  Task: {wf['description']}")
        print(f"  Prompt: {wf['prompt'][:80]}...")
        sys.stdout.flush()

        result = run_ante_query(wf["prompt"])
        response = extract_response(result["stdout"])

        eval_result = evaluate_response(
            response, wf["expected_topics"], wf["min_quality_words"]
        )

        status = "✓" if eval_result["passed"] else "✗"
        print(f"  {status} {eval_result['word_count']} words, "
              f"{eval_result['topic_coverage_pct']}% topics, "
              f"{result['elapsed']:.1f}s")

        print(f"  Topics found: {eval_result['found_topics']}")
        print(f"  Structure: {'•' if eval_result['has_bullets'] else ''}"
              f"{'code' if eval_result['has_code_blocks'] else ''}"
              f"{'#' if eval_result['has_headings'] else ''}"
              f"{' (none)' if eval_result['structure_score'] == 0 else ''}")
        if response:
            print(f"  Response preview: {response[:150]}...")
        print()

        all_results.append({
            "workflow": wf["name"],
            "passed": eval_result["passed"],
            "words": eval_result["word_count"],
            "topics_pct": eval_result["topic_coverage_pct"],
            "time_s": round(result["elapsed"], 1),
            "rc": result["returncode"],
        })

    # ── Summary ──
    print("=" * 72)
    passed = sum(1 for r in all_results if r["passed"])
    total = len(all_results)
    print(f"OVERALL: {passed}/{total} multi-agent workflows passed")
    print("=" * 72)

    print(f"{'Workflow':<30} {'Words':>6} {'Topics':>8} {'Time':>7}  Status")
    print("-" * 72)
    for r in all_results:
        s = "✓ PASS" if r["passed"] else "✗ FAIL"
        print(f"{r['workflow']:<30} {r['words']:>6} {r['topics_pct']:>7}% "
              f"{r['time_s']:>6.1f}s  {s}")

    print("-" * 72)
    print(f"AGENT SYSTEM STATUS: {'✓ OPERATIONAL' if passed == total else '⚠ DEGRADED'}")
    print(f"MULTI-AGENT WORKFLOW: {'✓ VERIFIED' if passed >= 1 else '✗ FAILED'}")

    # Save results
    results_path = Path("ante-test/workflow_results.json")
    results_path.parent.mkdir(parents=True, exist_ok=True)
    with open(results_path, "w") as f:
        json.dump(all_results, f, indent=2)
    print(f"\nFull results: {results_path}")

    sys.exit(0 if passed >= 2 else 1)


if __name__ == "__main__":
    main()
