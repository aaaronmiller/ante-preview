//! Error types for the Ante extensibility system.
//!
//! Uses `thiserror` derives for consistent error reporting.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur during hook execution.
#[derive(Error, Debug)]
pub enum HookError {
    /// The hook command timed out.
    #[error("Hook timed out after {0}ms")]
    Timeout(u64),

    /// The hook process exited with a non-zero code.
    #[error("Hook exited with code {code}: {stderr}")]
    NonZeroExit {
        code: i32,
        stderr: String,
    },

    /// The hook output could not be parsed as a valid decision.
    #[error("Failed to parse hook decision: {0}")]
    ParseError(String),

    /// The hook process could not be spawned.
    #[error("Failed to spawn hook process: {0}")]
    SpawnError(String),

    /// The hook returned an unrecognized decision type.
    #[error("Unrecognized hook decision: {0}")]
    UnrecognizedDecision(String),

    /// Maximum hook nesting depth exceeded.
    #[error("Hook nesting depth exceeded (max {0})")]
    NestingDepthExceeded(u32),
}

/// Errors that can occur during MCP server connection and invocation.
#[derive(Error, Debug)]
pub enum MCPError {
    /// Failed to start the MCP server process.
    #[error("Failed to start MCP server '{server}': {detail}")]
    ServerStart {
        server: String,
        detail: String,
    },

    /// The MCP server process crashed or was terminated.
    #[error("MCP server '{server}' crashed: {detail}")]
    ServerCrash {
        server: String,
        detail: String,
    },

    /// MCP protocol handshake failed.
    #[error("MCP handshake failed for '{server}': {detail}")]
    HandshakeFailed {
        server: String,
        detail: String,
    },

    /// The MCP tool was not found on the server.
    #[error("MCP tool '{tool}' not found on server '{server}'")]
    ToolNotFound {
        server: String,
        tool: String,
    },

    /// The MCP tool call returned an error.
    #[error("MCP tool '{server}/{tool}' returned error: {detail}")]
    ToolCallError {
        server: String,
        tool: String,
        detail: String,
    },

    /// JSON-RPC error from the MCP server.
    #[error("MCP JSON-RPC error from '{server}': code={code} message={message}")]
    JsonRpcError {
        server: String,
        code: i64,
        message: String,
    },

    /// The server requires OAuth authentication.
    #[error("MCP server '{server}' requires authentication")]
    NeedsAuth {
        server: String,
    },

    /// Connection timeout.
    #[error("MCP connection to '{server}' timed out after {timeout_secs}s")]
    Timeout {
        server: String,
        timeout_secs: u64,
    },
}

/// Errors that can occur during model routing.
#[derive(Error, Debug)]
pub enum RouterError {
    /// No models are configured in the pool.
    #[error("No models configured in model pool")]
    EmptyPool,

    /// No model could satisfy the capability requirement.
    #[error("No model found with capability_score >= {min_score} for complexity '{task_complexity}'")]
    NoSuitableModel {
        task_complexity: String,
        min_score: u32,
    },

    /// All models in the pool are disabled.
    #[error("All models in the pool are disabled")]
    AllDisabled,

    /// No fallback model available.
    #[error("No fallback model available after {attempts} attempt(s)")]
    NoFallback {
        attempts: u32,
    },
}

/// Errors related to context budget tracking.
#[derive(Error, Debug)]
pub enum BudgetError {
    /// Token budget exceeded.
    #[error("Token budget exceeded: {current}/{limit}")]
    TokenBudgetExceeded {
        current: u64,
        limit: u64,
    },

    /// Cost budget exceeded.
    #[error("Cost budget exceeded: ${current:.4}/${limit:.4}")]
    CostBudgetExceeded {
        current: f64,
        limit: f64,
    },
}

/// Errors related to settings loading and validation.
#[derive(Error, Debug)]
pub enum SettingsError {
    /// The settings file was not found at the expected path.
    #[error("Settings file not found at {0}")]
    FileNotFound(PathBuf),

    /// The settings file could not be parsed.
    #[error("Failed to parse settings: {0}")]
    ParseError(String),

    /// A required field was missing.
    #[error("Missing required setting: {0}")]
    MissingField(String),

    /// An invalid value was provided.
    #[error("Invalid setting value for '{field}': {detail}")]
    InvalidValue {
        field: String,
        detail: String,
    },
}
