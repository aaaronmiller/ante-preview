# Security Audit Report

**Phase 11, T063 — Security audit of extensibility features**

Date: 2026-05-20
Scope: Hooks, MCP, sub-agents, memory, HITL approval, inter-agent communication

## Audit Methodology

1. **Code review** — Manual inspection of all security-relevant code paths
2. **Pattern coverage** — Verify blocklist and HITL risk patterns cover real threats
3. **Bypass analysis** — Test whether PermissionRequest can be bypassed via MCP or hooks
4. **Privilege escalation** — Check sub-agent isolation and tool access boundaries

---

## 1. Default Blocklist Coverage

The `block-danger.sh` script installed by `init.rs` covers these patterns:

| Pattern | Risk | Status |
|---------|------|--------|
| `rm -rf` (root dir) | Critical — filesystem destruction | ✅ Covered |
| `dd if=` | Critical — raw device write | ✅ Covered |
| `format`, `mkfs`, `mkswap` | Critical — partition destruction | ✅ Covered |
| `fdisk` | Critical — partition management | ✅ Covered |
| `> /dev/` | Critical — raw device write | ✅ Covered |
| `chmod 777 /` | Critical — world-writable root | ✅ Covered |
| `sudo ` | High — privilege escalation | ✅ Covered (HITL) |
| `rm -rf` (non-root) | High — bulk deletion | ✅ Covered (HITL) |
| `chmod `, `chown ` | High — permission changes | ✅ Covered (HITL) |
| `curl \| bash`, `wget -O - \| sh` | Critical — remote code execution | ✅ Covered (HITL) |
| `apt/dnf remove` | High — package removal | ✅ Covered (HITL) |

**Gap**: `docker run --privileged`, `kubectl exec`, and similar container escape
patterns are not explicitly covered. These are rare but high-impact.
**Mitigation**: Falls back to High risk via the `sudo`/`chmod`-level patterns,
but container escapes can happen without `sudo` if the Docker socket is accessible.

## 2. HITL Bypass Analysis

### Scenario A: Bypass via MCP tools
An MCP server's tool could perform dangerous operations through its external channel
(e.g., an MCP server that wraps `rm` with a clean API). Since the MCP tool name won't
contain "Bash", the default `sensitiveTools` list won't flag it.

**Risk**: Medium
**Mitigation**: Users can add `mcp__*` to `sensitiveTools` or add patterns matching
known dangerous MCP tool names. The HITL `classify()` method checks the concatenated
`"{tool} {input}"` string, so even MCP tools with dangerous inputs (e.g., containing
"rm -rf") would be classified as High via the input pattern match.

### Scenario B: Bypass via hook scripts
A PreToolUse command hook could execute the tool's action directly, bypassing the
event pipeline's permission check.

**Risk**: Low — the hook system runs *before* the decision pipeline, not after.
Permission checks happen at the event dispatch level after hooks return Allow.
A malicious hook trying to skip permission would need to modify the event,
which is not supported by the current hook interface (hooks return Allow/Deny only).

### Scenario C: Bypass via sub-agent
A sub-agent could be given destructive tools without HITL oversight.

**Risk**: Low — sub-agents use the same tool permission pipeline as the main agent.
The `AgentContext` is shared, and `check_hitl()` is called before any tool execution.

### Scenario D: Direct memory file manipulation
The memory store is a plain JSON file at `~/.ante/memory.db` (current) or SQLite.

**Risk**: Low — local file permissions (`~/.ante/` is `700` by default on first run).
The file is not encrypted; a local attacker with filesystem access could read memories.
**Mitigation**: Document that `~/.ante/` should have restrictive permissions.

## 3. Sub-Agent Isolation

| Dimension | Status | Notes |
|-----------|--------|-------|
| Filesystem access | ⚠️ Shared | Sub-agents share the same filesystem namespace |
| Tool access | ✅ Restricted | Tool sets are per-agent (frontmatter `tools:` field) |
| Process isolation | ❌ None | Sub-agents run in-process, not separate processes |
| Memory isolation | ⚠️ Shared | All agents share the same `MemoryStore` |

Sub-agents are primarily logical orchestrations (task decomposition → dispatch →
synthesis), not security boundaries. The current model assumes trusted sub-agents.

**Recommendation**: For untrusted sub-agents, use a separate Ante process or container.

## 4. MCP Server Security

| Concern | Status | Notes |
|---------|--------|-------|
| Untrusted server code | ⚠️ Trust-based | MCP servers run as user-privilege subprocesses |
| Capability negotiation | ✅ Required | Handshake verifies server protocol version |
| Tool enumeration | ✅ At connect | All tools listed and registered before use |
| Malformed input handling | ✅ Timeout | `TimeoutConfig` prevents hung servers |
| Server restart limit | ✅ 3 max | `max_restarts` prevents infinite crash loops |

**Risk**: An MCP server with access to network/disk could perform operations
outside Ante's permission model.
**Mitigation**: Servers run as the same user — no sandboxing. Treat MCP servers
as trusted extensions.

## 5. Broker (Inter-Agent) Security

| Concern | Status | Notes |
|---------|--------|-------|
| Socket access control | ⚠️ File perms | Unix socket at `~/.ante/run/intercom.sock` |
| Message authentication | ❌ None | Any local process can connect to the socket |
| Message confidentiality | ❌ None | Messages are plaintext on the socket |
| Denial of service | ⚠️ Connection limit | No explicit limit beyond system resources |

**Risk**: Any process on the same machine can connect to the broker socket,
send/receive messages, and impersonate agents.
**Mitigation**: The socket file inherits `~/.ante/` directory permissions (700).
On a multi-user system, this is insufficient. For production multi-tenant use,
add TLS or Unix credential passing (SO_PEERCRED).

## 6. Supply Chain

| Concern | Status | Notes |
|---------|--------|-------|
| MCP server installation | ⚠️ External | Servers installed via npm/pip/etc. outside Ante |
| Hook scripts | ✅ Local | Only from `~/.ante/hooks/` (user-writable) |
| Settings file integrity | ⚠️ No signing | JSON settings are plaintext, no integrity check |
| Binary integrity | ❌ Not verified | No binary signing or checksum verification |

## 7. Findings Summary

| # | Severity | Finding | Recommendation |
|---|----------|---------|---------------|
| F1 | Medium | MCP tools bypass sensitiveTools by default | Add `mcp__*` glob support to sensitive tool matching |
| F2 | Medium | Broker socket accessible to local processes | Add Unix credential validation (SO_PEERCRED) |
| F3 | Low | Sub-agents lack process isolation | Document as intended design; containerize for untrusted use |
| F4 | Low | No memory encryption | Add optional encryption for memory store |
| F5 | Info | No container escape protection in HITL patterns | Add patterns: `docker --privileged`, `kubectl exec`, `nsenter` |
| F6 | Info | Settings file integrity not verified | Sign settings file with HMAC or detached signature |

## 8. Conclusion

The extensibility system provides defense-in-depth through layered security:
- **HITL approval** catches dangerous operations at the tool invocation boundary
- **Hook system** provides pre-execution blocking for known dangerous patterns
- **Sub-agent tool restrictions** limit blast radius
- **MCP timeout and restart limits** prevent resource exhaustion

For a local-first, single-user agent runtime, the current security posture is
**adequate**. For multi-tenant or enterprise deployment, findings F1, F2, and F4
should be addressed before production use.
