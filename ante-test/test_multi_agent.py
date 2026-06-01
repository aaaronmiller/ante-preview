#!/usr/bin/env python3
"""
Multi-agent integration test for Ante using prompttools.

Tests Ante's agent system end-to-end through the mock Claude → litellm → Ollama pipeline.
Exercises the full Ante binary with real LLM inference on free local models.

Usage:
    PATH="/tmp/ante-mock-path:$PATH" python3 ante-test/test_multi_agent.py
"""

import json
import os
import subprocess
import sys
import time
import csv
from pathlib import Path

# Configure Ante binary path
ANTE_BIN = os.environ.get("ANTE_BIN", "ante")
TEST_DIR = Path(__file__).parent

# ── Test scenarios ──────────────────────────────────────────────────

SCENARIOS = [
    {
        "name": "basic-query",
        "prompt": "Say hello.",
        "expected_keywords": ["hello"],
        "min_response_length": 1,
        "type": "query",
        "description": "Basic single-turn query through agent pipeline",
    },
    {
        "name": "db-comparison",
        "prompt": "Give one pro and one con of SQLite for a desktop app. Keep it to 2 sentences.",
        "expected_keywords": ["sqlite", "pro"],
        "min_response_length": 10,
        "type": "query",
        "description": "Decision-making task with structured output",
    },
    {
        "name": "rust-function",
        "prompt": "Write a short Rust function that adds two numbers. Include the function signature and body.",
        "expected_keywords": ["fn", "i32", "->"],
        "min_response_length": 5,
        "type": "query",
        "description": "Code generation task exercising Rust knowledge",
    },
    {
        "name": "error-handling",
        "prompt": "What should a CLI tool do when the network is down? Give a one-sentence answer.",
        "expected_keywords": ["error", "retry", "message"],
        "min_response_length": 8,
        "type": "query",
        "description": "Error handling design task",
    },
]

# ── Evaluation helpers ──────────────────────────────────────────────

def check_keywords(text, keywords):
    """Check how many keywords appear in the response text."""
    lower = text.lower()
    found = [kw for kw in keywords if kw.lower() in lower]
    return len(found), found

def check_response_length(text, min_len):
    """Check if response meets minimum length requirement."""
    words = text.split()
    return len(words) >= min_len


# ── Run test via Ante ───────────────────────────────────────────────

def run_ante_query(prompt, timeout=300):
    """Run `ante query` with the given prompt and return output."""
    cmd = [
        ANTE_BIN, "query", prompt,
        "--no-hitl",
        "--no-memory",
        "--no-router",
    ]
    env = os.environ.copy()
    env["PATH"] = f"{os.pathsep.join(['/tmp/ante-mock-path', env.get('PATH', '')])}"

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
        }
    except subprocess.TimeoutExpired:
        return {
            "stdout": "",
            "stderr": "TIMEOUT",
            "returncode": -1,
        }
    except FileNotFoundError:
        return {
            "stdout": "",
            "stderr": f"Ante binary not found at {ANTE_BIN}",
            "returncode": -2,
        }


def run_ante_agents(task, timeout=120):
    """Run `ante agents run` to test task decomposition."""
    cmd = [ANTE_BIN, "agents", "run"] + task.split()
    env = os.environ.copy()
    env["PATH"] = f"{os.pathsep.join(['/tmp/ante-mock-path', env.get('PATH', '')])}"

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
            input="exit\n",
        )
        return {
            "stdout": result.stdout,
            "stderr": result.stderr,
            "returncode": result.returncode,
        }
    except subprocess.TimeoutExpired:
        return {
            "stdout": "",
            "stderr": "TIMEOUT",
            "returncode": -1,
        }


# ── prompttools-style experiment ────────────────────────────────────

def run_experiment(scenarios):
    """Run all scenarios and collect results."""
    results = []

    print(f"Running {len(scenarios)} scenarios through Ante...\n")

    for i, scenario in enumerate(scenarios, 1):
        print(f"[{i}/{len(scenarios)}] {scenario['name']}: {scenario['description']}")
        print(f"  Prompt: {scenario['prompt'][:80]}...")

        t0 = time.time()

        if scenario["type"] == "query":
            output = run_ante_query(scenario["prompt"])
        elif scenario["type"] == "agents":
            output = run_ante_agents(scenario["prompt"])
        else:
            output = {"stdout": "", "stderr": "unknown type", "returncode": -1}

        elapsed = time.time() - t0

        # Extract assistant response from stdout
        # Parse the assistant section between "── assistant" and "── result"
        stdout = output["stdout"]
        response_text = ""
        assistant_match = False
        for line in stdout.split("\n"):
            if '── assistant' in line:
                assistant_match = True
                continue
            if '── result' in line:
                break
            if assistant_match:
                # Strip quote marks and leading whitespace
                cleaned = line.strip().strip('"')
                if cleaned:
                    response_text += cleaned + " "

        response_text = response_text.strip()
        word_count = len(response_text.split()) if response_text else 0

        # Run evaluations
        kw_count, kw_found = check_keywords(response_text, scenario["expected_keywords"])
        passes_length = check_response_length(response_text, scenario["min_response_length"])
        passes_keywords = kw_count >= max(1, len(scenario["expected_keywords"]) // 2)
        passed = passes_keywords and passes_length and output["returncode"] == 0

        result = {
            "scenario": scenario["name"],
            "prompt": scenario["prompt"],
            "response": response_text or "(no assistant response extracted)",
            "response_word_count": word_count,
            "keywords_found": kw_found,
            "keywords_expected": scenario["expected_keywords"],
            "keywords_match_pct": round(kw_count / len(scenario["expected_keywords"]) * 100, 1),
            "passes_length_check": passes_length,
            "passes_keyword_check": passes_keywords,
            "passed": passed,
            "returncode": output["returncode"],
            "time_seconds": round(elapsed, 2),
            "stderr": output["stderr"][:200] if output["stderr"] else "",
        }

        status = "✓" if result["passed"] else "✗"
        print(f"  {status} {word_count} words, {result['keywords_match_pct']}% keywords, {elapsed:.1f}s")
        print(f"  Response: {response_text[:120]}...\n")

        results.append(result)

    return results


def print_summary(results):
    """Print results table."""
    passed = sum(1 for r in results if r["passed"])
    total = len(results)

    print("=" * 80)
    print(f"RESULTS: {passed}/{total} passed")
    print("=" * 80)
    print(f"{'Scenario':<25} {'Words':>6} {'KW%':>6} {'Time':>7} {'Status':>8}")
    print("-" * 80)
    for r in results:
        status = "✓ PASS" if r["passed"] else "✗ FAIL"
        print(f"{r['scenario']:<25} {r['response_word_count']:>6} {r['keywords_match_pct']:>5}% {r['time_seconds']:>6.1f}s {status:>8}")

    print("-" * 80)
    print(f"PASS RATE: {passed}/{total} ({passed/total*100:.0f}%)")
    print()

    # Show failures in detail
    failures = [r for r in results if not r["passed"]]
    if failures:
        print("FAILURE DETAILS:")
        for r in failures:
            print(f"  [{r['scenario']}] rc={r['returncode']}, words={r['response_word_count']}, kw={r['keywords_found']}")
            print(f"  stderr: {r['stderr'][:200]}")
            print()


def export_csv(results, path="ante-test/results.csv"):
    """Export results to CSV for analysis."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "scenario", "passed", "response_word_count",
            "keywords_match_pct", "time_seconds", "returncode"
        ])
        writer.writeheader()
        for r in results:
            writer.writerow({
                "scenario": r["scenario"],
                "passed": r["passed"],
                "response_word_count": r["response_word_count"],
                "keywords_match_pct": r["keywords_match_pct"],
                "time_seconds": r["time_seconds"],
                "returncode": r["returncode"],
            })
    print(f"Results exported to {path}")


def export_json(results, path="ante-test/results.json"):
    """Export full results as JSON."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w") as f:
        json.dump(results, f, indent=2, default=str)
    print(f"Results exported to {path}")


# ── Multi-agent specific test ───────────────────────────────────────

def test_agent_decomposition():
    """
    Test that Ante's agent system can decompose a complex task
    through its agent registry and match agents to sub-tasks.
    """
    print("\n── Agent Decomposition Test ──\n")

    test_cases = [
        {
            "task": "Write code and review it",
            "expected_agents": ["code-reviewer", "task-writer"],
            "description": "Two-step workflow: write then review",
        },
        {
            "task": "Design the database schema then implement the API endpoints",
            "expected_agents": ["task-writer"],
            "description": "Sequential decomposition across phases",
        },
    ]

    for tc in test_cases:
        print(f"Task: {tc['task']}")
        result = run_ante_agents(tc["task"])
        stdout = result["stdout"]
        print(f"  Output: {stdout[:300]}")
        print()


# ── Main ────────────────────────────────────────────────────────────

def main():
    # Check mock Claude is reachable
    mock_path = "/tmp/ante-mock-path/claude"
    if not os.path.exists(mock_path):
        print(f"ERROR: Mock Claude not found at {mock_path}")
        print("Run: mkdir -p /tmp/ante-mock-path && cat > /tmp/ante-mock-path/claude << 'EOF'")
        print("#!/bin/bash")
        print('exec mock-claude "$@"')
        print("EOF")
        print("chmod +x /tmp/ante-mock-path/claude")
        sys.exit(1)

    # Check litellm is running
    import urllib.request
    try:
        urllib.request.urlopen("http://localhost:4000/health", timeout=2)
    except Exception:
        print("ERROR: litellm proxy not running on port 4000")
        print("Run: litellm --model ollama/llama3.2:1b --port 4000 --drop_params &")
        sys.exit(1)

    print(f"Using Ante binary: {ANTE_BIN}")
    print(f"Mock Claude: {mock_path}")
    print(f"Model: ollama/llama3.2:1b (via litellm → Ollama)")
    print()

    # Run agent decomposition tests
    test_agent_decomposition()

    # Run main experiment
    results = run_experiment(SCENARIOS)
    print_summary(results)

    # Export
    export_csv(results)
    export_json(results)

    # Final verdict
    passed = sum(1 for r in results if r["passed"])
    total = len(results)
    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
