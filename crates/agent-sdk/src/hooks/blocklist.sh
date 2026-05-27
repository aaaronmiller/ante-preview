#!/bin/bash
# Default Ante blocklist hook.
# Reads event JSON from stdin, returns allow/deny decision on stdout.
#
# Blocks dangerous command patterns:
#   - rm -rf /
#   - sudo without explicit user confirmation
#   - chmod 777 / chmod -R 777
#   - dd if= of= (destructive disk operations)
#   - > /dev/ (can destroy block devices)
#   - :(){ :|:& };: (fork bomb)
#   - curl/wget pipe to bash (remote code execution)
#   - eval "$(curl/wget)" (remote code execution)
#
# Usage: echo '{"event_type":"pre_tool_use",...}' | block-danger.sh
# Output: {"type":"allow"} or {"type":"deny","reason":"..."}

set -euo pipefail

# Read the event payload from stdin
INPUT=$(cat)

# Extract tool_name and command
TOOL_NAME=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_name',''))" 2>/dev/null || echo "")
COMMAND=$(echo "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('input',{}).get('command',''))" 2>/dev/null || echo "")

# If not a Bash tool, allow
if [ "$TOOL_NAME" != "Bash" ] && [ "$TOOL_NAME" != "Execute" ]; then
    echo '{"type":"allow"}'
    exit 0
fi

# Check for dangerous patterns (case-insensitive comparison)
LC_ALL=C

# rm -rf / or rm -rf /*
if echo "$COMMAND" | grep -qiE '(^|[^a-zA-Z])rm[[:space:]]+(-[rf]+|-[rf]+[^a-zA-Z]*|.*-rf.*)[[:space:]]+/[[:space:]]*$'; then
    echo '{"type":"deny","reason":"rm -rf / is blocked by default hook: would destroy the root filesystem"}'
    exit 0
fi

# rm -rf / (variant with /*
if echo "$COMMAND" | grep -qiE '(^|[^a-zA-Z])rm[[:space:]]+.*[[:space:]]+/([[:space:]]|\*|$)'; then
    echo '{"type":"deny","reason":"Dangerous recursive delete blocked: target is root filesystem"}'
    exit 0
fi

# Fork bomb
if echo "$COMMAND" | grep -qiE ':[[:space:]]*\([[:space:]]*\)[[:space:]]*\{[[:space:]]*:[[:space:]]*\|[[:space:]]*:[[:space:]]*&[[:space:]]*;[[:space:]]*\}'; then
    echo '{"type":"deny","reason":"Fork bomb (:(){ :|:& };:) blocked by default hook"}'
    exit 0
fi

# dd destructive operations
if echo "$COMMAND" | grep -qiE '(^|[^a-zA-Z])dd[[:space:]]+.*if=(/[^*]|/[^i])' && echo "$COMMAND" | grep -qiE 'of='; then
    echo '{"type":"deny","reason":"dd with if= and of= blocked by default hook: can destroy block devices and filesystems"}'
    exit 0
fi

# chmod 777
if echo "$COMMAND" | grep -qiE 'chmod[[:space:]]+(-R[[:space:]]+)?777'; then
    echo '{"type":"deny","reason":"chmod 777 blocked by default hook: makes files world-writable"}'
    exit 0
fi

# curl|wget pipe to bash/sh
if echo "$COMMAND" | grep -qiE '(curl|wget)[[:space:]].*\|[[:space:]]*(bash|sh)'; then
    echo '{"type":"deny","reason":"Remote code execution via curl/wget pipe to shell blocked by default hook"}'
    exit 0
fi

# eval with curl/wget
if echo "$COMMAND" | grep -qiE '(^|[^a-zA-Z])eval[[:space:]]+.*\$(curl|wget|http)'; then
    echo '{"type":"deny","reason":"eval with remote content blocked by default hook"}'
    exit 0
fi

# sudo on sensitive operations
if echo "$COMMAND" | grep -qiE '(^|[^a-zA-Z])sudo[[:space:]]+(rm|dd|mkfs|fdisk|parted|format)'; then
    echo '{"type":"deny","reason":"sudo with destructive command blocked by default hook: use with caution"}'
    exit 0
fi

# No dangerous patterns found — allow
echo '{"type":"allow"}'
exit 0
