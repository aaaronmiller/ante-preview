//! Hook execution primitives.
//!
//! A `HookExecutor` runs a single `HookDefinition` against an event
//! payload and returns a `HookOutput` with the resulting decision.

pub mod command;
pub mod mcp_tool;
pub mod prompt;
pub mod registry;

use ante_protocol_shape::{EventPayload, EventType, HookDecision, HookDefinition};
use thiserror::Error;

/// Output from a single hook execution.
#[derive(Debug, Clone)]
pub struct HookOutput {
    /// The decision returned by the hook.
    pub hook_decision: HookDecision,
    /// Human-readable name for the hook that was executed.
    pub hook_name: String,
}

/// Errors that can occur during hook execution.
#[derive(Debug, Error)]
pub enum HookExecError {
    #[error("Command hook execution failed: {0}")]
    Command(#[from] command::CommandHookError),

    #[error("Unknown hook type")]
    UnknownHookType,
}

/// Executes a single hook definition and returns the result.
#[derive(Debug, Clone)]
pub struct HookExecutor {
    definition: HookDefinition,
}

impl HookExecutor {
    pub fn new(definition: HookDefinition, _event_type: EventType) -> Self {
        Self { definition }
    }

    /// Execute the hook against the given event payload.
    ///
    /// The payload is serialized to JSON and passed to the hook via stdin.
    /// The hook is expected to return a `HookDecision` JSON object on stdout.
    pub async fn execute(&self, payload: &EventPayload) -> Result<HookOutput, HookExecError> {
        let hook_name = match &self.definition {
            HookDefinition::Command { command, .. } => format!("cmd:{command}"),
            HookDefinition::Prompt { prompt, .. } => format!("prompt:{:.40}", prompt.replace('\n', " ")),
            HookDefinition::McpTool { server, tool, .. } => format!("mcp:{server}/{tool}"),
        };

        match &self.definition {
            HookDefinition::Command { command, args, timeout_ms } => {
                let output = command::run_command_hook(
                    command,
                    args,
                    payload,
                    *timeout_ms,
                )
                .await?;
                Ok(HookOutput { hook_decision: output, hook_name })
            }
            HookDefinition::McpTool { server, tool, args } => {
                match mcp_tool::run_mcp_tool_hook(server, tool, args, payload).await {
                    Ok(decision) => Ok(HookOutput { hook_decision: decision, hook_name }),
                    Err(_) => {
                        // MCP tool hook not available yet — fall through to Allow
                        Ok(HookOutput {
                            hook_decision: HookDecision::Allow,
                            hook_name,
                        })
                    }
                }
            }
            HookDefinition::Prompt { .. } => {
                // Prompt hooks will be implemented in a follow-up phase.
                // For now, return Allow.
                Ok(HookOutput {
                    hook_decision: HookDecision::Allow,
                    hook_name,
                })
            }
        }
    }
}
