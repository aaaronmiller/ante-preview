//! Hook execution primitives.
//!
//! A `HookExecutor` runs a single `HookDefinition` against an event
//! payload and returns a `HookOutput` with the resulting decision.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub mod command;
pub mod mcp_tool;
pub mod prompt;
pub mod registry;

use ante_protocol_shape::{EventPayload, EventType, HookDecision, HookDefinition};
use serde_json::Value;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Callback type aliases for LLM / MCP / SubAgent invocation
// ---------------------------------------------------------------------------

/// LLM invocation callback used by prompt hooks.
///
/// Receives the formatted prompt string, returns the LLM's text response.
pub type InvokeLlm = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync,
>;

/// MCP tool invocation callback used by MCP-tool hooks.
///
/// Receives (server_name, tool_name, arguments), returns a JSON result
/// on success or an error string on failure.
pub type InvokeMcp = Arc<
    dyn Fn(String, String, Value) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

/// Sub-agent invocation callback used by sub-agent hooks.
///
/// Receives (agent_name, task_prompt), returns a JSON result with the
/// sub-agent's output on success or an error string on failure.
pub type InvokeSubAgent = Arc<
    dyn Fn(String, String) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

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

    #[error("Prompt hook execution failed: {0}")]
    Prompt(#[from] prompt::PromptHookError),

    #[error("MCP tool hook execution failed: {0}")]
    McpTool(#[from] mcp_tool::McpToolHookError),

    #[error("Sub-agent hook execution failed: {0}")]
    SubAgent(String),

    #[error("Unknown hook type")]
    UnknownHookType,
}

/// Executes a single hook definition and returns the result.
///
/// The `llm_callback` and `mcp_callback` parameters are used by
/// `Prompt` and `McpTool` hook types respectively.  Pass `None` for
/// either to skip that hook type (returns `HookDecision::Allow`).
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
    /// For `Prompt` hooks: the `llm_callback` is called with the
    /// formatted prompt template.  For `McpTool` hooks: the
    /// `mcp_callback` is called with (server, tool, args).  For
    /// `SubAgent` hooks: the `subagent_callback` is called with
    /// (agent_name, formatted_task).  Command hooks run directly and
    /// need no callback.
    pub async fn execute(
        &self,
        payload: &EventPayload,
        llm_callback: Option<&InvokeLlm>,
        mcp_callback: Option<&InvokeMcp>,
        subagent_callback: Option<&InvokeSubAgent>,
    ) -> Result<HookOutput, HookExecError> {
        let hook_name = match &self.definition {
            HookDefinition::Command { command, .. } => format!("cmd:{command}"),
            HookDefinition::Prompt { prompt, .. } => {
                format!("prompt:{:.40}", prompt.replace('\n', " "))
            }
            HookDefinition::McpTool { server, tool, .. } => format!("mcp:{server}/{tool}"),
            HookDefinition::SubAgent { agent_name, .. } => {
                format!("subagent:{agent_name}")
            }
        };

        match &self.definition {
            HookDefinition::Command {
                command,
                args,
                timeout_ms,
            } => {
                let output =
                    command::run_command_hook(command, args, payload, *timeout_ms).await?;
                Ok(HookOutput {
                    hook_decision: output,
                    hook_name,
                })
            }
            HookDefinition::McpTool {
                server,
                tool,
                args,
            } => {
                if let Some(cb) = mcp_callback {
                    match mcp_tool::run_mcp_tool_hook(cb, server, tool, args, payload).await {
                        Ok(decision) => Ok(HookOutput {
                            hook_decision: decision,
                            hook_name,
                        }),
                        Err(e) => {
                            // Log the error but allow the operation through
                            eprintln!("[ante] MCP tool hook error: {e}");
                            Ok(HookOutput {
                                hook_decision: HookDecision::Allow,
                                hook_name,
                            })
                        }
                    }
                } else {
                    // No MCP callback provided — allow
                    Ok(HookOutput {
                        hook_decision: HookDecision::Allow,
                        hook_name,
                    })
                }
            }
            HookDefinition::SubAgent {
                agent_name,
                task,
            } => {
                if let Some(cb) = subagent_callback {
                    // Format the task template with event data
                    let event_json = serde_json::to_string_pretty(payload)
                        .unwrap_or_else(|_| "{}".to_string());
                    let event_type_s = payload.event_type().to_string();
                    let formatted_task = task
                        .replace("{event_json}", &event_json)
                        .replace("{event_type}", &event_type_s);

                    match cb(agent_name.clone(), formatted_task).await {
                        Ok(result_value) => {
                            // Serialize the Value to a JSON string so we can
                            // parse it as a HookDecision.
                            let json_str = serde_json::to_string(&result_value)
                                .unwrap_or_else(|_| "{}".to_string());
                            let decision = crate::hooks::prompt::parse_decision(&json_str)
                                .unwrap_or(HookDecision::Allow);
                            Ok(HookOutput {
                                hook_decision: decision,
                                hook_name,
                            })
                        }
                        Err(e) => {
                            eprintln!("[ante] Sub-agent hook error: {e}");
                            Ok(HookOutput {
                                hook_decision: HookDecision::Allow,
                                hook_name,
                            })
                        }
                    }
                } else {
                    // No sub-agent callback provided — allow
                    Ok(HookOutput {
                        hook_decision: HookDecision::Allow,
                        hook_name,
                    })
                }
            }
            HookDefinition::Prompt {
                prompt: prompt_template,
                model,
            } => {
                if let Some(cb) = llm_callback {
                    // Wrap the Arc callback into a closure that borrows the
                    // prompt string by converting to owned and calling the Arc.
                    let cb_clone = cb.clone();
                    let pt = prompt_template.clone();
                    let m = model.clone();
                    let pl = payload.clone();

                    // Build a one-shot callable with explicit return type
                    let llm_call = move |prompt: &str| -> Pin<Box<dyn Future<Output = String> + Send>> {
                        let owned = prompt.to_string();
                        let cb = cb_clone.clone();
                        Box::pin(async move { cb(owned).await })
                    };

                    match prompt::run_prompt_hook(&pt, &m, &pl, llm_call).await {
                        Ok(decision) => Ok(HookOutput {
                            hook_decision: decision,
                            hook_name,
                        }),
                        Err(e) => {
                            eprintln!("[ante] Prompt hook error: {e}");
                            Ok(HookOutput {
                                hook_decision: HookDecision::Allow,
                                hook_name,
                            })
                        }
                    }
                } else {
                    // No LLM callback provided — allow
                    Ok(HookOutput {
                        hook_decision: HookDecision::Allow,
                        hook_name,
                    })
                }
            }
        }
    }
}
