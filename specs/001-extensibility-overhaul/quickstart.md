# Quickstart: Ante Extensibility Overhaul

## 1. Creating Your First Hook

Create a security hook that blocks dangerous `rm` commands:

```bash
# Create the hooks directory
mkdir -p ~/.ante/hooks

# Write a hook script
cat > ~/.ante/hooks/block-dangerous-rm.sh << 'EOF'
#!/bin/bash
# Read event from stdin
INPUT=$(cat)

# Extract the command
COMMAND=$(echo "$INPUT" | grep -o '"command":"[^"]*"' | cut -d'"' -f4)

# Check for dangerous patterns
if echo "$COMMAND" | grep -qE 'rm\s+(-rf\s+)?\/'; then
  echo '{"decision":"deny","reason":"Blocked: rm on root directory"}'
  exit 2
fi

# Allow everything else
echo '{"decision":"allow"}'
exit 0
EOF

chmod +x ~/.ante/hooks/block-dangerous-rm.sh
```

Register it in `~/.ante/settings.json`:

```json
{
  "hooks": {
    "rules": [
      {
        "eventTypes": ["PreToolUse"],
        "toolNamePattern": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "~/.ante/hooks/block-dangerous-rm.sh"
          }
        ]
      }
    ]
  }
}
```

Restart Ante. Now every `Bash` command is checked against your hook.

## 2. Adding an MCP Server

Add a web search tool to Ante:

```json
{
  "mcpServers": [
    {
      "name": "web-search",
      "command": "npx",
      "args": ["-y", "@antigma/ante-web-search"],
      "autoStart": true
    }
  ]
}
```

Restart Ante. The MCP server connects automatically, and its tools are
available under the `mcp__` prefix. Use them like any built-in tool.

## 3. Defining a Sub-Agent

Create `~/.ante/agents/security-reviewer.md`:

```markdown
---
name: security-reviewer
description: Reviews code for security vulnerabilities
tools: [Read, Grep, Glob]
---

You are a security expert. Review the provided code for:
- Injection vulnerabilities (SQL, command, XSS)
- Insecure deserialization
- Hardcoded secrets
- Permission misconfiguration

Be thorough but practical. Focus on exploitable issues.
```

The task decomposition engine automatically discovers and uses this agent
when it detects a security review request.

## 4. Using Persistent Memory

Memory works automatically with built-in JSON file persistence:

```json
{
  "memory": {
    "dbPath": "~/.ante/memory.db",
    "maxContextMemories": 10,
    "autoIndex": true
  }
}
```

Built-in hooks handle memory automatically:
- **SessionStart** — loads relevant past context into the session
- **PostToolUse** on edits — saves outputs as tagged memory entries
- **PostToolUseFailure** — records failures for future awareness

The agent can also manually search or add memories using the `memory_read`
and `memory_write` tools.

## 5. Configuring the Model Router

Add models to your pool for automatic selection:

```json
{
  "modelPool": [
    {
      "name": "Local Gemma-4",
      "provider": "local",
      "modelId": "gemma-4-9b-it",
      "capabilityScore": 40,
      "costPer1kInput": 0,
      "costPer1kOutput": 0,
      "enabled": true
    },
    {
      "name": "Qwen 3.6-27B",
      "provider": "openrouter",
      "modelId": "qwen/qwen-3.6-27b",
      "capabilityScore": 75,
      "costPer1kInput": 0.00015,
      "costPer1kOutput": 0.0006,
      "enabled": true
    }
  ],
  "contextBudget": {
    "maxTokens": 1000000,
    "maxCostUsd": 0.50
  }
}
```

Simple edits use the free local model. Complex refactors use the paid
capable model. You stay within budget automatically.

## 6. Importing Claude Code Hooks

If you already have Claude Code hooks at `.claude/settings.json`, enable
compatibility mode:

```json
{
  "claudeCompat": {
    "mergeClaudeSettings": true,
    "translateEventNames": true
  }
}
```

Ante reads your existing configuration and runs those hooks natively.
No migration needed.

## 7. Next Steps

- Add more hooks in `~/.ante/hooks/`
- Browse available MCP servers at [mcp.so](https://mcp.so)
- Define specialized sub-agents for your workflows
- Set your budget limits based on typical usage
- Run `ante --help` for CLI options related to these features
