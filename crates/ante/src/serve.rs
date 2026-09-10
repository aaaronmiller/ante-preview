//! `ante serve`: host sessions over the JSONL Op/Event protocol on stdio.
//!
//! Reads [`OpMsg`] lines on stdin, writes [`EventMsg`] lines on stdout.
//! Session persistence reuses [`SessionManager`]; turns execute through the
//! Claude backend, connected lazily on first input. Ops with no meaningful
//! mapping on this host answer with [`Evt::Error`] rather than failing
//! silently.
//!
//! Limits of this host (by design, not omission): resume restores session
//! identity and title but does not re-inject history into the backend;
//! cross-process resume needs a persisted host-side session table.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use ante_protocol_shape::{
    event_msg, Evt, Id, ModelSpec, Op, OpMsg, ProviderSpec, SessionEndReason, SessionInfo,
    SessionRequest, SessionUpdate, TurnEndStatus, Usage,
};
use ante_sdk::claude::{Claude, ClaudeMessage, ClaudeOptions};
use ante_sdk::sessions::SessionManager;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Transport is always stdio; the flag exists for upstream-CLI compatibility.
pub async fn run_stdio(sessions_root: PathBuf, cwd: PathBuf) -> io::Result<()> {
    let mut host = Host::new(sessions_root, cwd);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: OpMsg = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                emit(&mut out, Evt::Error(format!("invalid op: {e}")), None).await?;
                continue;
            }
        };
        let parent = Some(msg.id);
        let (events, exit) = host.handle_op(msg.op).await;
        for event in events {
            emit(&mut out, event, parent).await?;
        }
        if exit {
            break;
        }
    }
    Ok(())
}

async fn emit(
    out: &mut tokio::io::Stdout,
    event: Evt,
    parent: Option<Id>,
) -> io::Result<()> {
    let line = serde_json::to_string(&event_msg(event, parent))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    out.write_all(line.as_bytes()).await?;
    out.write_all(b"\n").await?;
    out.flush().await
}

/// Protocol session id (rendered) -> (manager session id, last info).
type LiveMap = HashMap<String, (String, SessionInfo)>;

pub struct Host {
    sessions: SessionManager,
    default_cwd: PathBuf,
    default_model: String,
    live: LiveMap,
    active_proto: Option<String>,
    claude: Option<Claude>,
    turning: bool,
}

impl Host {
    pub fn new(sessions_root: PathBuf, default_cwd: PathBuf) -> Self {
        Self {
            sessions: SessionManager::new(sessions_root),
            default_cwd,
            default_model: "claude-sonnet-4-5".to_string(),
            live: HashMap::new(),
            active_proto: None,
            claude: None,
            turning: false,
        }
    }

    fn info_for(&self, proto_id: &Id, manager_title: Option<String>, req: &SessionRequest) -> SessionInfo {
        let model = req.model.clone().unwrap_or_else(|| self.default_model.clone());
        SessionInfo {
            model: ModelSpec {
                id: model,
                display_name: None,
                description: None,
                temperature: None,
                top_p: None,
                top_k: None,
                max_tokens: None,
                stop_sequences: None,
                context_limit: None,
                effort: None,
                supported_efforts: None,
                support_vision: None,
                weight_class: None,
            },
            provider: ProviderSpec {
                id: req.provider.clone().unwrap_or_else(|| "anthropic".into()),
                display_name: req.provider.clone().unwrap_or_else(|| "anthropic".into()),
                base_url: String::new(),
            },
            session_id: proto_id.clone(),
            cwd: req.cwd.clone().unwrap_or_else(|| self.default_cwd.clone()),
            permission_mode: req
                .permission_mode
                .clone()
                .unwrap_or_default(),
            skills: Vec::new(),
            subagents: Vec::new(),
            title: manager_title.or_else(|| req.title.clone()),
        }
    }

    fn active_manager_id(&self) -> Option<String> {
        self.active_proto
            .as_ref()
            .and_then(|p| self.live.get(p))
            .map(|(manager_id, _)| manager_id.clone())
    }

    fn end_active(&mut self, reason: SessionEndReason) -> Vec<Evt> {
        let Some(proto) = self.active_proto.take() else {
            return Vec::new();
        };
        let Some((_, info)) = self.live.remove(&proto) else {
            return Vec::new();
        };
        // Manager-active tracks host-active: every start/end flows through here.
        let _ = self.sessions.end(0);
        vec![Evt::SessionEnd {
            session_id: info.session_id,
            reason,
            usage: Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            },
        }]
    }

    /// Handle one op. Returns emitted events plus whether to exit.
    pub async fn handle_op(&mut self, op: Op) -> (Vec<Evt>, bool) {
        match op {
            Op::StartSession(req) => {
                let mut events = self.end_active(SessionEndReason::Replaced);
                let cwd = req.cwd.clone().unwrap_or_else(|| self.default_cwd.clone());
                match self
                    .sessions
                    .start(&cwd, req.provider.as_deref(), req.model.as_deref())
                {
                    Ok(manager_id) => {
                        if req.title.is_some() {
                            let _ = self.sessions.set_title(req.title.clone());
                        }
                        let proto_id = Id::ses();
                        let info = self.info_for(&proto_id, req.title.clone(), &req);
                        self.live.insert(
                            proto_id.to_string(),
                            (manager_id, info.clone()),
                        );
                        self.active_proto = Some(proto_id.to_string());
                        events.push(Evt::SessionStart(Box::new(info)));
                    }
                    Err(e) => events.push(Evt::Error(format!("cannot start session: {e}"))),
                }
                (events, false)
            }
            Op::UpdateSession(update) => {
                if self.active_manager_id().is_none() {
                    return (vec![Evt::Error("no active session".into())], false);
                }
                let mut notes = Vec::new();
                if update.model.is_some() || update.permission_mode.is_some() {
                    notes.push("only title updates are currently applied");
                }
                match self.apply_title_update(&update) {
                    Ok(Some(info)) => {
                        let mut events: Vec<Evt> = notes
                            .iter()
                            .map(|n| Evt::Info((*n).to_string()))
                            .collect();
                        events.push(Evt::SessionUpdated(Box::new(info)));
                        (events, false)
                    }
                    Ok(None) => (vec![Evt::Error("no active session".into())], false),
                    Err(e) => (vec![Evt::Error(format!("cannot rename: {e}"))], false),
                }
            }
            Op::UserInput(text) => self.handle_user_input(text).await,
            Op::SlashCommand { name, args } => {
                if name == "rename" {
                    let trimmed = args.trim();
                    let title = if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed.to_string())
                    };
                    return self.rename_active(title).await;
                }
                (
                    vec![Evt::Error(format!("unsupported slash command: /{name}") )],
                    false,
                )
            }
            Op::ResumeSession { session_id } => {
                let key = session_id.to_string();
                if let Some((_, info)) = self.live.get(&key) {
                    self.active_proto = Some(key);
                    (vec![Evt::SessionStart(Box::new(info.clone()))], false)
                } else {
                    (
                        vec![Evt::Error(
                            "unknown session: resume needs a session started on this connection"
                                .into(),
                        )],
                        false,
                    )
                }
            }
            Op::Interrupt => {
                if !self.turning {
                    return (vec![Evt::Error("no active turn".into())], false);
                }
                self.turning = false;
                if let Some(ref mut claude) = self.claude {
                    let _ = claude.interrupt().await;
                }
                (
                    vec![Evt::Info("turn interrupted".into())],
                    false,
                )
            }
            Op::Shutdown => {
                let mut events = self.end_active(SessionEndReason::Shutdown);
                events.push(Evt::Goodbye);
                (events, true)
            }
            other => (
                vec![Evt::Error(format!(
                    "unsupported op: {}",
                    op_tag(&other)
                ))],
                false,
            ),
        }
    }

    /// Apply a title update to the active session; returns refreshed info.
    fn apply_title_update(&mut self, update: &SessionUpdate) -> io::Result<Option<SessionInfo>> {
        let Some(proto) = self.active_proto.clone() else {
            return Ok(None);
        };
        let Some((_, info)) = self.live.get_mut(&proto) else {
            return Ok(None);
        };
        if let Some(raw) = update.title.clone() {
            let trimmed = raw.trim();
            let title = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
            self.sessions.set_title(title.clone())?;
            info.title = title;
            Ok(Some(info.clone()))
        } else {
            Ok(Some(info.clone()))
        }
    }

    /// Shared rename path for `Op::UpdateSession` title and `/rename`.
    async fn rename_active(&mut self, title: Option<String>) -> (Vec<Evt>, bool) {
        let Some(proto) = self.active_proto.clone() else {
            return (vec![Evt::Error("no active session".into())], false);
        };
        let live_title = title.clone();
        if self.sessions.set_title(title).is_err() {
            return (vec![Evt::Error("no active session".into())], false);
        }
        if let Some((_, info)) = self.live.get_mut(&proto) {
            info.title = live_title;
            (
                vec![Evt::SessionUpdated(Box::new(info.clone()))],
                false,
            )
        } else {
            (vec![Evt::Error("no active session".into())], false)
        }
    }

    async fn handle_user_input(&mut self, text: String) -> (Vec<Evt>, bool) {
        if self.active_manager_id().is_none() {
            return (vec![Evt::Error("no active session".into())], false);
        }
        if self.claude.is_none() {
            let mut options = ClaudeOptions::default();
            options.model = Some(self.default_model.clone());
            match Claude::connect(options).await {
                Ok(client) => self.claude = Some(client),
                Err(e) => {
                    return (
                        vec![Evt::Error(format!("backend unavailable: {e}"))],
                        false,
                    )
                }
            }
        }
        let turn_id = Id::new("turn");
        let mut events = vec![Evt::TurnStart {
            turn_id: turn_id.clone(),
        }];
        self.turning = true;
        let _ = self
            .sessions
            .record_user_message(&text);
        let outcome = self
            .claude
            .as_mut()
            .expect("connected above")
            .query(text)
            .await;
        self.turning = false;
        match outcome {
            Ok(response) => {
                let text = response
                    .messages
                    .iter()
                    .filter_map(|m| match m {
                        ClaudeMessage::Assistant(a) => a.text(),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = self.sessions.record_assistant_message(
                    serde_json::Value::String(text.clone()),
                    None,
                    None,
                );
                events.push(Evt::AgentMessage(text));
                events.push(Evt::TurnEnd {
                    turn_id,
                    status: TurnEndStatus::Completed,
                    steps: 1,
                });
            }
            Err(e) => events.push(Evt::TurnEnd {
                turn_id,
                status: TurnEndStatus::Error {
                    kind: None,
                    headline: e.to_string(),
                    details: Vec::new(),
                },
                steps: 0,
            }),
        }
        (events, false)
    }
}

/// Short tag naming an op variant for "unsupported" errors.
fn op_tag(op: &Op) -> &'static str {
    match op {
        Op::StartSession(_) => "StartSession",
        Op::UpdateSession(_) => "UpdateSession",
        Op::Interrupt => "Interrupt",
        Op::UserInput(_) => "UserInput",
        Op::ShellInput(_) => "ShellInput",
        Op::Steer(_) => "Steer",
        Op::ApprovalResponse { .. } => "ApprovalResponse",
        Op::SlashCommand { .. } => "SlashCommand",
        Op::ResumeSession { .. } => "ResumeSession",
        Op::RegisterLocalProvider { .. } => "RegisterLocalProvider",
        Op::RestoreLocalProvider => "RestoreLocalProvider",
        Op::Compact { .. } => "Compact",
        Op::ContextReport => "ContextReport",
        Op::Goal(_) => "Goal",
        Op::AmbientPhrase { .. } => "AmbientPhrase",
        Op::AmbientSuggestion { .. } => "AmbientSuggestion",
        Op::Shutdown => "Shutdown",
    }
}

/// Resolve `sessions_root` + `cwd` for `ante serve` and run the stdio host.
pub async fn run_serve() -> io::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let sessions_root = PathBuf::from(home).join(".ante").join("sessions");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    run_stdio(sessions_root, cwd).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ante_protocol_shape::SessionUpdate;

    fn test_host() -> (tempfile::TempDir, Host) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sessions");
        let host = Host::new(root, PathBuf::from("/home/user/project"));
        (dir, host)
    }

    fn start_req() -> SessionRequest {
        SessionRequest {
            model: None,
            provider: None,
            permission_mode: None,
            system_prompt: None,
            append_system_prompt: None,
            tools: None,
            include_tools: None,
            exclude_tools: None,
            cwd: None,
            effort: None,
            enable_auto_memory: None,
            short_prompt: None,
            no_skills: None,
            save_session: None,
            title: Some("Serve test".into()),
        }
    }

    #[tokio::test]
    async fn start_then_shutdown_lifecycle() {
        let (_dir, mut host) = test_host();
        let (events, exit) = host.handle_op(Op::StartSession(start_req())).await;
        assert!(!exit);
        assert_eq!(events.len(), 1);
        match &events[0] {
            Evt::SessionStart(info) => assert_eq!(info.title.as_deref(), Some("Serve test")),
            other => panic!("expected SessionStart, got {other:?}"),
        }
        let (events, exit) = host.handle_op(Op::Shutdown).await;
        assert!(exit);
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], Evt::SessionEnd { .. }));
        assert!(matches!(events[1], Evt::Goodbye));
        let sessions = host.sessions.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title.as_deref(), Some("Serve test"));
    }

    #[tokio::test]
    async fn update_title_patches_session() {
        let (_dir, mut host) = test_host();
        host.handle_op(Op::StartSession(start_req())).await;
        let update = SessionUpdate {
            title: Some("New name".into()),
            ..Default::default()
        };
        let (events, _) = host.handle_op(Op::UpdateSession(update)).await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            Evt::SessionUpdated(info) => assert_eq!(info.title.as_deref(), Some("New name")),
            other => panic!("expected SessionUpdated, got {other:?}"),
        }
        // Empty text clears the title, per the protocol rule.
        let clear = SessionUpdate {
            title: Some("   ".into()),
            ..Default::default()
        };
        let (events, _) = host.handle_op(Op::UpdateSession(clear)).await;
        match &events[0] {
            Evt::SessionUpdated(info) => assert!(info.title.is_none()),
            other => panic!("expected SessionUpdated, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rename_slash_and_unknown_session_errors() {
        let (_dir, mut host) = test_host();
        // No session yet: everything errors honestly.
        let (events, _) = host
            .handle_op(Op::SlashCommand {
                name: "rename".into(),
                args: "x".into(),
            })
            .await;
        assert!(matches!(events[0], Evt::Error(_)));
        let (events, _) = host.handle_op(Op::UserInput("hi".into())).await;
        assert!(matches!(events[0], Evt::Error(_)));
        // Start, then rename via slash command.
        host.handle_op(Op::StartSession(start_req())).await;
        let (events, _) = host
            .handle_op(Op::SlashCommand {
                name: "rename".into(),
                args: "Slash title".into(),
            })
            .await;
        match &events[0] {
            Evt::SessionUpdated(info) => {
                assert_eq!(info.title.as_deref(), Some("Slash title"))
            }
            other => panic!("expected SessionUpdated, got {other:?}"),
        }
        let (events, _) = host
            .handle_op(Op::SlashCommand {
                name: "bogus".into(),
                args: String::new(),
            })
            .await;
        assert!(matches!(events[0], Evt::Error(_)));
        // Unmapped ops name themselves instead of failing silently.
        let (events, _) = host.handle_op(Op::ShellInput("ls".into())).await;
        match &events[0] {
            Evt::Error(msg) => assert!(msg.contains("ShellInput")),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resume_unknown_session_errors() {
        let (_dir, mut host) = test_host();
        let (events, _) = host
            .handle_op(Op::ResumeSession {
                session_id: Id::ses(),
            })
            .await;
        assert!(matches!(events[0], Evt::Error(_)));
    }
}
