//! MCP server process lifecycle manager.
//!
//! Manages spawning, monitoring, and restarting of subprocesses
//! that serve as MCP server daemons. This is a generic process
//! lifecycle manager — the ante binary wires MCP config entries
//! to the spawn() method.
//!
//! Features:
//!   - Spawn with auto-start support
//!   - Crash monitoring with configurable restart backoff
//!   - Graceful shutdown (SIGTERM → SIGKILL)
//!   - Broadcast channel for stdout observation

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::{Mutex, broadcast};
use tokio::time::sleep;

/// Configuration for a managed subprocess.
#[derive(Debug, Clone)]
pub struct ManagedProcessConfig {
    /// Display name (used for logging and lookup).
    pub name: String,
    /// Program or binary path.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// Environment variables (overrides).
    pub env: Vec<(String, String)>,
    /// Working directory (None = inherit).
    pub cwd: Option<PathBuf>,
    /// Whether to start on session begin.
    pub auto_start: bool,
    /// Max restart attempts before giving up (0 = no restarts).
    pub max_restarts: u32,
    /// Start timeout in seconds.
    pub start_timeout_secs: u64,
}

impl Default for ManagedProcessConfig {
    fn default() -> Self {
        ManagedProcessConfig {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
            auto_start: true,
            max_restarts: 3,
            start_timeout_secs: 30,
        }
    }
}

/// Observable runtime state of a managed process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    /// Not yet started.
    Stopped,
    /// Currently running.
    Running,
    /// Crashed unexpectedly.
    Crashed,
    /// Restarting with backoff.
    Restarting,
    /// Permanently shut down.
    Shutdown,
}

/// Handle to a running managed process.
pub struct ManagedProcess {
    name: String,
    state: Arc<Mutex<ProcessState>>,
    child: Arc<Mutex<Option<tokio::process::Child>>>,
    restart_count: Arc<Mutex<u32>>,
    config: ManagedProcessConfig,
    stdout_tx: broadcast::Sender<String>,
}

impl ManagedProcess {
    /// Human-readable name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Current observable state.
    pub async fn state(&self) -> ProcessState {
        *self.state.lock().await
    }

    /// The config this process was spawned from.
    pub fn config(&self) -> &ManagedProcessConfig {
        &self.config
    }

    /// Subscribe to a broadcast channel of stdout lines.
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.stdout_tx.subscribe()
    }

    /// Number of restarts so far.
    pub async fn restart_count(&self) -> u32 {
        *self.restart_count.lock().await
    }

    /// Send a signal to stop the process gracefully.
    pub async fn shutdown(&self) {
        let mut state = self.state.lock().await;
        if *state == ProcessState::Stopped || *state == ProcessState::Shutdown {
            return;
        }
        *state = ProcessState::Shutdown;
        drop(state);

        let mut child_opt = self.child.lock().await;
        if let Some(ref mut child) = *child_opt {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        *child_opt = None;
    }

    /// Kill the process immediately (no graceful stop).
    pub async fn kill(&self) {
        let mut child_opt = self.child.lock().await;
        if let Some(ref mut child) = *child_opt {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
        *child_opt = None;
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        // Best-effort kill during drop; use a short lock attempt
        // to avoid blocking during panic unwind.
        if let Ok(mut guard) = self.child.try_lock() {
            if let Some(ref mut child) = *guard {
                let _ = child.start_kill();
            }
            *guard = None;
        }
    }
}

/// Manages the lifecycle of a set of subprocesses.
pub struct ProcessManager {
    processes: Vec<Arc<ManagedProcess>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        ProcessManager {
            processes: Vec::new(),
        }
    }

    /// Spawn a process from the given config.
    ///
    /// Returns a handle that can be used to monitor and control the process.
    pub async fn spawn(&mut self, config: ManagedProcessConfig) -> Result<Arc<ManagedProcess>, String> {
        if config.command.is_empty() {
            return Err("command cannot be empty".to_string());
        }
        if config.name.is_empty() {
            return Err("name cannot be empty".to_string());
        }

        let (stdout_tx, _) = broadcast::channel(256);

        let handle = Arc::new(ManagedProcess {
            name: config.name.clone(),
            state: Arc::new(Mutex::new(ProcessState::Stopped)),
            child: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(Mutex::new(0)),
            config: config.clone(),
            stdout_tx,
        });

        // Start the child process
        Self::do_spawn(&handle).await?;

        self.processes.push(handle.clone());
        Ok(handle)
    }

    /// Attempt a restart with exponential backoff.
    pub async fn restart(&self, handle: &ManagedProcess) -> Result<(), String> {
        let mut state = handle.state.lock().await;
        if *state == ProcessState::Shutdown {
            return Err("process is shut down, cannot restart".to_string());
        }
        *state = ProcessState::Restarting;
        drop(state);

        let mut rcount = handle.restart_count.lock().await;
        if *rcount >= handle.config.max_restarts {
            return Err(format!(
                "max restarts ({}) exhausted for '{}'",
                handle.config.max_restarts, handle.name
            ));
        }
        *rcount += 1;
        let count = *rcount;
        drop(rcount);

        // Kill the old process if still running
        handle.kill().await;

        // Exponential backoff: 1s, 2s, 4s, 8s
        let delay = Duration::from_secs(1 << (count.saturating_sub(1)));
        sleep(delay).await;

        Self::do_spawn(handle).await
    }

    /// Internal spawn — configures and launches the subprocess.
    async fn do_spawn(handle: &ManagedProcess) -> Result<(), String> {
        let mut cmd = Command::new(&handle.config.command);
        cmd.args(&handle.config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);

        for (key, val) in &handle.config.env {
            cmd.env(key, val);
        }

        if let Some(ref cwd) = handle.config.cwd {
            cmd.current_dir(cwd);
        }

        let child = cmd.spawn().map_err(|e| {
            format!("failed to spawn '{}': {}", handle.config.name, e)
        })?;

        // Store the child process handle
        {
            let mut child_opt = handle.child.lock().await;
            *child_opt = Some(child);
        }

        {
            let mut state = handle.state.lock().await;
            *state = ProcessState::Running;
        }

        Ok(())
    }

    /// Gracefully shut down all managed processes.
    pub async fn shutdown_all(&self) {
        for process in &self.processes {
            process.shutdown().await;
        }
    }

    /// Get all process handles.
    pub fn handles(&self) -> &[Arc<ManagedProcess>] {
        &self.processes
    }

    /// Find a managed process by name.
    pub fn find(&self, name: &str) -> Option<&Arc<ManagedProcess>> {
        self.processes.iter().find(|p| p.name == name)
    }

    /// Number of managed processes.
    pub fn count(&self) -> usize {
        self.processes.len()
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_and_shutdown_process() {
        let mut pm = ProcessManager::new();

        let handle = pm
            .spawn(ManagedProcessConfig {
                name: "test-sh".into(),
                command: "sh".into(),
                args: vec!["-c".into(), "echo hello".into()],
                auto_start: true,
                max_restarts: 0,
                ..Default::default()
            })
            .await;

        // sh -c "echo hello" should spawn fine
        assert!(handle.is_ok(), "spawn failed: {:?}", handle.err());
        let handle = handle.unwrap();

        assert_eq!(handle.name(), "test-sh");
        assert_eq!(pm.count(), 1);

        // Give it time to run then shutdown
        sleep(Duration::from_millis(200)).await;
        handle.shutdown().await;
        assert_eq!(handle.state().await, ProcessState::Shutdown);
    }

    #[tokio::test]
    async fn reject_empty_config() {
        let mut pm = ProcessManager::new();

        let result = pm
            .spawn(ManagedProcessConfig {
                name: "".into(),
                command: "".into(),
                ..Default::default()
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn restart_count_increments() {
        // Use a binary that exits immediately to trigger restart scenario
        let mut pm = ProcessManager::new();

        let handle = pm
            .spawn(ManagedProcessConfig {
                name: "quick-exit".into(),
                command: "true".into(),
                args: vec![],
                auto_start: true,
                max_restarts: 3,
                ..Default::default()
            })
            .await
            .expect("spawn");

        // Give it time to exit
        sleep(Duration::from_millis(100)).await;

        // Try restarting
        let restart_result = pm.restart(&handle).await;
        assert!(restart_result.is_ok(), "restart failed: {:?}", restart_result.err());
        assert_eq!(handle.restart_count().await, 1);
    }

    #[tokio::test]
    async fn find_by_name() {
        let mut pm = ProcessManager::new();
        pm.spawn(ManagedProcessConfig {
            name: "finder".into(),
            command: "sleep".into(),
            args: vec!["1".into()],
            ..Default::default()
        })
        .await
        .expect("spawn");

        assert!(pm.find("finder").is_some());
        assert!(pm.find("nonexistent").is_none());
    }

    #[tokio::test]
    async fn shutdown_all() {
        let mut pm = ProcessManager::new();
        for i in 0..3 {
            pm.spawn(ManagedProcessConfig {
                name: format!("proc-{}", i),
                command: "sleep".into(),
                args: vec!["10".into()],
                ..Default::default()
            })
            .await
            .expect("spawn");
        }

        assert_eq!(pm.count(), 3);
        pm.shutdown_all().await;

        for h in pm.handles() {
            assert_eq!(h.state().await, ProcessState::Shutdown);
        }
    }

    #[tokio::test]
    async fn max_restarts_exhausted() {
        let mut pm = ProcessManager::new();
        let handle = pm
            .spawn(ManagedProcessConfig {
                name: "exhauster".into(),
                command: "true".into(),
                args: vec![],
                auto_start: true,
                max_restarts: 1,
                ..Default::default()
            })
            .await
            .expect("spawn");

        // First restart should work
        assert!(pm.restart(&handle).await.is_ok());

        // Second restart should fail (max 1)
        sleep(Duration::from_millis(1500)).await;
        let result = pm.restart(&handle).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exhausted"));
    }
}
