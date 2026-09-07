//! Command hook executor — runs shell commands as hook scripts.
//!
//! The event payload is serialized to JSON and piped to the command's
//! stdin. The command is expected to write a `HookDecision` JSON object
//! to stdout and exit with code 0.

use std::time::Duration;

use ante_protocol_shape::{EventPayload, HookDecision};
use serde_json::Value;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Debug, Error)]
pub enum CommandHookError {
    #[error("Failed to spawn hook process: {0}")]
    Spawn(std::io::Error),

    #[error("Hook stdin unavailable")]
    NoStdin,

    #[error("Hook stdout unavailable")]
    NoStdout,

    #[error("Failed to write payload to hook stdin: {0}")]
    StdinWrite(std::io::Error),

    #[error("Failed to read hook stdout: {0}")]
    StdoutRead(std::io::Error),

    #[error("Hook timed out after {0}ms")]
    Timeout(u64),

    #[error("Hook exited with code {code}: {stderr}")]
    NonZeroExit {
        code: i32,
        stderr: String,
    },

    #[error("Hook output is not valid JSON decision: {detail} (output: {output})")]
    InvalidDecision {
        detail: String,
        output: String,
    },

    #[error("Hook process killed (no output)")]
    Killed,
}

/// Run a command hook: spawn the process, pipe the event payload via stdin,
/// read and parse the decision from stdout.
pub async fn run_command_hook(
    command: &str,
    args: &[String],
    payload: &EventPayload,
    timeout_ms: Option<u64>,
) -> Result<HookDecision, CommandHookError> {
    let payload_bytes =
        serde_json::to_vec(payload).map_err(|e| CommandHookError::InvalidDecision {
            detail: e.to_string(),
            output: String::new(),
        })?;

    let mut child = Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(CommandHookError::Spawn)?;

    let mut stdin = child.stdin.take().ok_or(CommandHookError::NoStdin)?;
    let stdout = child.stdout.take().ok_or(CommandHookError::NoStdout)?;
    let stderr = child.stderr.take().ok_or(CommandHookError::NoStdout)?;

    // Write payload to stdin
    use tokio::io::AsyncWriteExt;
    stdin
        .write_all(&payload_bytes)
        .await
        .map_err(CommandHookError::StdinWrite)?;
    stdin
        .shutdown()
        .await
        .map_err(CommandHookError::StdinWrite)?;
    // Drop stdin explicitly to close the pipe, signalling EOF to the child.
    // Without this, the child's cat/read will hang waiting for more input.
    drop(stdin);

    // Read stdout and stderr concurrently
    let stdout_handle = tokio::spawn(read_stdout(stdout));
    let stderr_handle = tokio::spawn(read_stderr(stderr));

    // Wait for the process with optional timeout
    let exit_status = if let Some(ms) = timeout_ms {
        match timeout(Duration::from_millis(ms), child.wait()).await {
            Ok(status) => status.map_err(CommandHookError::Spawn)?,
            Err(_elapsed) => {
                // Timeout — kill and return error
                let _ = child.start_kill().ok();
                let _ = child.wait().await;
                return Err(CommandHookError::Timeout(ms));
            }
        }
    } else {
        child.wait().await.map_err(CommandHookError::Spawn)?
    };

    let stdout_str = stdout_handle
        .await
        .unwrap_or(Ok(String::new()))
        .map_err(CommandHookError::StdoutRead)?;
    let stderr_str = stderr_handle.await.unwrap_or_default();

    let code = exit_status.code().unwrap_or(-1);

    if code != 0 {
        return Err(CommandHookError::NonZeroExit {
            code,
            stderr: stderr_str.trim().to_string(),
        });
    }

    // Parse the last non-empty line of stdout as a HookDecision
    parse_hook_output(&stdout_str)
}

async fn read_stdout(mut stdout: tokio::process::ChildStdout) -> Result<String, std::io::Error> {
    let mut reader = tokio::io::BufReader::new(&mut stdout);
    let mut output = String::new();
    reader.read_to_string(&mut output).await?;
    Ok(output)
}

async fn read_stderr(mut stderr: tokio::process::ChildStderr) -> String {
    let mut reader = tokio::io::BufReader::new(&mut stderr);
    let mut output = String::new();
    let _ = reader.read_to_string(&mut output).await;
    output
}

/// Parse the last non-empty line of hook stdout as a HookDecision.
fn parse_hook_output(stdout: &str) -> Result<HookDecision, CommandHookError> {
    let last_line = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .last()
        .unwrap_or_default();

    if last_line.is_empty() {
        // Empty output = Allow
        return Ok(HookDecision::Allow);
    }

    let trimmed = last_line.trim();

    // Try parsing as a HookDecision (tagged JSON)
    if let Ok(decision) = serde_json::from_str::<HookDecision>(trimmed) {
        return Ok(decision);
    }

    // Try parsing as raw JSON value and infer
    if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
        // If it has a "type" field, use it directly
        if let Some(ty) = val.get("type").and_then(|v| v.as_str()) {
            match ty {
                "allow" => return Ok(HookDecision::Allow),
                "deny" => {
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Deny { reason });
                }
                "modify" => {
                    let modified_input = val.get("modifiedInput").or_else(|| val.get("modified_input")).cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Modify { modified_input, reason });
                }
                _ => {}
            }
        }

        // If it has "allow": true/false
        if let Some(allowed) = val.get("allow").and_then(|v| v.as_bool()) {
            if allowed {
                if let Some(input) = val.get("modifiedInput").or_else(|| val.get("modified_input")) {
                    let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                    return Ok(HookDecision::Modify {
                        modified_input: input.clone(),
                        reason,
                    });
                }
                return Ok(HookDecision::Allow);
            } else {
                let reason = val.get("reason").and_then(|v| v.as_str()).map(String::from);
                return Ok(HookDecision::Deny { reason });
            }
        }
    }

    Err(CommandHookError::InvalidDecision {
        detail: "expected a HookDecision JSON object".into(),
        output: trimmed.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_allow_output() {
        let result = parse_hook_output(r#"{"type":"allow"}"#);
        assert!(result.is_ok());
        assert!(result.unwrap().is_allowed());
    }

    #[test]
    fn parse_deny_output() {
        let result = parse_hook_output(r#"{"type":"deny","reason":"nope"}"#);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_allowed());
    }

    #[test]
    fn parse_empty_output_as_allow() {
        let result = parse_hook_output("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_allowed());
    }

    #[test]
    fn parse_allow_boolean_output() {
        let result = parse_hook_output(r#"{"allow":false,"reason":"blocked"}"#);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_allowed());
    }

    #[test]
    fn parse_invalid_output() {
        let result = parse_hook_output("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_multiline_picks_last_line() {
        let result = parse_hook_output("ignore this line\n{\"type\":\"allow\"}");
        assert!(result.is_ok());
        assert!(result.unwrap().is_allowed());
    }
}
