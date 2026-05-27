# Contract: Event Payload Schema

## Purpose

Defines the JSON payload shape passed to hooks via stdin (command hooks),
LLM context (prompt hooks), or as arguments (MCP tool hooks). This schema
must be stable across Ante versions — new fields MUST be optional.

## Base Schema (all events)

```json
{
  "session_id": "01JQZ3H8KX...",
  "event_type": "PreToolUse",
  "timestamp": "2026-05-19T14:30:00Z",
  "cwd": "/home/user/project",
  "transcript_path": "/tmp/ante-sessions/ses_abc123.md",
  "ante_version": "0.1.0",
  "ante_subagent_id": null
}
```

## Event-Specific Payloads

### PreToolUse / PostToolUse

```json
{
  "session_id": "...",
  "event_type": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": {
    "command": "rm -rf /tmp/build"
  },
  "cwd": "/home/user/project",
  "transcript_path": "...",
  "ante_version": "0.1.0"
}
```

`PostToolUse` additionally includes:
```json
{
  "tool_output": {
    "exit_code": 0,
    "stdout": "build complete\n",
    "stderr": ""
  }
}
```

### PostToolUseFailure

```json
{
  "event_type": "PostToolUseFailure",
  "tool_name": "Bash",
  "tool_input": { "command": "make" },
  "error": {
    "message": "Command timed out after 30000ms",
    "code": "TIMEOUT",
    "exit_code": null
  }
}
```

### UserPromptSubmit

```json
{
  "event_type": "UserPromptSubmit",
  "tool_name": null,
  "user_message": "refactor the auth module to use JWT"
}
```

### SessionStart / SessionEnd

```json
{
  "event_type": "SessionStart",
  "tool_name": null,
  "session_started_at": "2026-05-19T14:30:00Z",
  "project_dir": "/home/user/project"
}
```

`SessionEnd` additionally includes:
```json
{
  "session_duration_seconds": 3600,
  "tools_called": 47,
  "total_tokens_used": 125000
}
```

### PermissionRequest

```json
{
  "event_type": "PermissionRequest",
  "tool_name": "Bash",
  "tool_input": { "command": "deploy --prod" },
  "risk_level": "high",
  "reason": "Tool 'Bash' is designated as sensitive"
}
```

## Hook Decision Schema

Hooks return a JSON decision on stdout (command hooks) or as LLM output
(prompt hooks):

### Allow

```json
{
  "decision": "allow"
}
```

### Deny

```json
{
  "decision": "deny",
  "reason": "Destructive command blocked by security policy"
}
```

### Modify

```json
{
  "decision": "modify",
  "modified_input": {
    "command": "ls -la /tmp/build"
  }
}
```

## Exit Codes (Command Hooks)

| Exit Code | Meaning |
|-----------|---------|
| 0 | Allow — use stdout JSON for decision |
| 1 | Allow with error logging — use stdout JSON, log stderr |
| 2 | Deny — use stdout JSON for reason |
| 3 | Error — hook failed, agent logs warning and continues |
| Any other | Treat as error, agent logs and continues |
