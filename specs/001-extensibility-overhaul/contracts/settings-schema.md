# Contract: `settings.json` Schema

## Purpose

`~/.ante/settings.json` is the single configuration file for all Ante
extensibility features. This document defines its schema.

## Full Schema

```json
{
  "$schema": "https://docs.antigma.ai/schemas/settings-v1.json",

  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "~/.ante/hooks/validate-bash.sh",
            "timeout_ms": 5000
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": ".*",
        "hooks": [
          {
            "type": "mcp-tool",
            "mcp_server": "memory",
            "mcp_tool": "memory_get_context",
            "timeout_ms": 10000
          }
        ]
      }
    ],
    "PermissionRequest": [
      {
        "matcher": ".*",
        "hooks": [
          {
            "type": "prompt",
            "prompt": "Evaluate if this tool call is safe. Tool: {tool_name}. Input: {tool_input}",
            "timeout_ms": 30000
          }
        ]
      }
    ]
  },

  "mcpServers": {
    "memory": {
      "command": "ante",
      "args": ["mcp", "memory"],
      "transport": "stdio",
      "auto_start": true
    },
    "web-search": {
      "command": "npx",
      "args": ["-y", "@antigma/ante-web-search"],
      "transport": "stdio",
      "auto_start": true
    }
  },

  "agents": {
    "code-reviewer": {
      "description": "Reviews code diffs for bugs and style issues",
      "prompt": "You are a senior code reviewer...",
      "tools": ["Read", "Grep", "Glob"],
      "model": "local"
    },
    "documenter": {
      "description": "Writes documentation from source context",
      "prompt": "You are a technical writer...",
      "tools": ["Read", "Write", "Edit"],
      "max_turns": 10
    }
  },

  "modelPool": [
    {
      "name": "Local Gemma-4",
      "provider": "local",
      "model_id": "gemma-4-9b-it",
      "cost_per_1k_input": 0,
      "cost_per_1k_output": 0,
      "latency_tier": "fast",
      "capability_score": 40,
      "privacy_tier": "local"
    },
    {
      "name": "Qwen 3.6-27B",
      "provider": "openrouter",
      "model_id": "qwen/qwen-3.6-27b",
      "cost_per_1k_input": 0.00015,
      "cost_per_1k_output": 0.0006,
      "latency_tier": "medium",
      "capability_score": 75,
      "privacy_tier": "trusted"
    }
  ],

  "budget": {
    "max_tokens": 1000000,
    "max_cost_usd": 0.50,
    "warn_threshold_pct": 80
  },

  "claudeCompat": true
}
```

## Claude Code Compatibility

When `"claudeCompat": true` is set, Ante will also read
`.claude/settings.json` if present and merge its hook configurations.
The event name translation is:

| Claude Code Event | Ante Event |
|-------------------|------------|
| `PreToolUse` | `PreToolUse` |
| `PostToolUse` | `PostToolUse` |
| `UserPromptSubmit` | `UserPromptSubmit` |
| `SessionStart` | `SessionStart` |
| `SessionEnd` | `SessionEnd` |

Claude Code command hooks are executed identically — the JSON payload
schema matches (see [event-payload-schema.md](./event-payload-schema.md)).
