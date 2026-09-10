//! ACP session driver: dispatches JSON-RPC lines to sessions.
//!
//! The [`Backend`] trait is the execution seam: production runs turns through
//! [`ClaudeBackend`], tests drive the full protocol with [`ScriptBackend`].

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;

use ante_sdk::claude::{Claude, ClaudeMessage, ClaudeOptions};
use ante_sdk::sessions::SessionManager;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    CancelParams, NewSessionParams, PROTOCOL_VERSION, PromptParams, RpcId, RpcIncoming,
    RpcResponse, chunk_notification, codes, prompt_text,
};

/// Executes one prompt turn. Returns the assistant's answer text.
pub trait Backend {
    fn prompt(
        &mut self,
        text: String,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send;
    fn interrupt(&mut self) -> impl std::future::Future<Output = ()> + Send {
        async {}
    }
}

/// Production backend: one persistent Claude connection per adapter.
pub struct ClaudeBackend {
    client: Option<Claude>,
    model: Option<String>,
}

impl ClaudeBackend {
    pub fn new() -> Self {
        Self {
            client: None,
            model: None,
        }
    }
}

impl Default for ClaudeBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for ClaudeBackend {
    async fn prompt(&mut self, text: String) -> Result<String, String> {
        if self.client.is_none() {
            let mut options = ClaudeOptions::default();
            options.model = self.model.clone();
            self.client = Some(Claude::connect(options).await.map_err(|e| e.to_string())?);
        }
        let client = self.client.as_mut().expect("connected above");
        let response = client.query(text).await.map_err(|e| e.to_string())?;
        Ok(response
            .messages
            .iter()
            .filter_map(|m| match m {
                ClaudeMessage::Assistant(a) => a.text(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn interrupt(&mut self) {
        if let Some(ref mut client) = self.client {
            let _ = client.interrupt().await;
        }
    }
}

/// The adapter: owns sessions, routes lines, enforces the lifecycle.
pub struct Agent<B> {
    backend: B,
    sessions: SessionManager,
    default_cwd: PathBuf,
    initialized: bool,
    live: HashSet<String>,
    active_turns: HashSet<String>,
}

impl<B: Backend> Agent<B> {
    pub fn new(backend: B, sessions_root: PathBuf, default_cwd: PathBuf) -> Self {
        Self {
            backend,
            sessions: SessionManager::new(sessions_root),
            default_cwd,
            initialized: false,
            live: HashSet::new(),
            active_turns: HashSet::new(),
        }
    }

    /// Handle one input line; returns output lines (responses/notifications).
    pub async fn handle_line(&mut self, line: &str) -> Vec<String> {
        if line.trim().is_empty() {
            return Vec::new();
        }
        let incoming: RpcIncoming = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(e) => {
                return vec![
                    RpcResponse::err(None, codes::PARSE, format!("invalid JSON-RPC: {e}"))
                        .render(),
                ]
            }
        };
        let Some(id) = incoming.id.clone() else {
            // Client notifications need no answer.
            return Vec::new();
        };
        if incoming.jsonrpc != "2.0" {
            return vec![RpcResponse::err(
                Some(id),
                codes::INVALID_PARAMS,
                "jsonrpc must be \"2.0\"",
            )
            .render()];
        }
        match incoming.method.as_str() {
            "initialize" => self.handle_initialize(id, incoming.params),
            "session/new" => self.handle_new(id, incoming.params),
            "session/prompt" => self.handle_prompt(id, incoming.params).await,
            "session/cancel" => self.handle_cancel(id, incoming.params).await,
            _ => vec![
                RpcResponse::err(Some(id), codes::METHOD_NOT_FOUND, format!("unknown method: {}", incoming.method))
                    .render(),
            ],
        }
    }

    fn initialized_or(&self, id: &RpcId) -> Result<(), String> {
        if self.initialized {
            Ok(())
        } else {
            Err(RpcResponse::err(
                Some(id.clone()),
                codes::NOT_INITIALIZED,
                "send initialize first",
            )
            .render())
        }
    }

    /// Answer with our version; negotiation is the client's call.
    fn handle_initialize(&mut self, id: RpcId, _params: Value) -> Vec<String> {
        self.initialized = true;
        vec![RpcResponse::ok(
            Some(id),
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "agentCapabilities": { "loadSession": false, "auth": { "logout": false } },
            }),
        )
        .render()]
    }

    fn handle_new(&mut self, id: RpcId, params: Value) -> Vec<String> {
        if let Err(line) = self.initialized_or(&id) {
            return vec![line];
        }
        let parsed: NewSessionParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return vec![
                    RpcResponse::err(Some(id), codes::INVALID_PARAMS, format!("bad session/new params: {e}"))
                        .render(),
                ]
            }
        };
        let cwd = parsed.cwd.unwrap_or_else(|| self.default_cwd.clone());
        match self.sessions.start(&cwd, None, None) {
            Ok(session_id) => {
                self.live.insert(session_id.clone());
                vec![RpcResponse::ok(Some(id), json!({ "sessionId": session_id })).render()]
            }
            Err(e) => vec![
                RpcResponse::err(Some(id), codes::BACKEND, format!("cannot start session: {e}"))
                    .render(),
            ],
        }
    }

    async fn handle_prompt(&mut self, id: RpcId, params: Value) -> Vec<String> {
        if let Err(line) = self.initialized_or(&id) {
            return vec![line];
        }
        let parsed: PromptParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return vec![
                    RpcResponse::err(
                        Some(id),
                        codes::INVALID_PARAMS,
                        format!("bad session/prompt params: {e}"),
                    )
                    .render(),
                ]
            }
        };
        if !self.live.contains(&parsed.session_id) {
            return vec![
                RpcResponse::err(Some(id), codes::UNKNOWN_SESSION, "unknown session")
                    .render(),
            ];
        }
        let text = prompt_text(&parsed.prompt);
        if text.trim().is_empty() {
            return vec![
                RpcResponse::err(Some(id), codes::INVALID_PARAMS, "prompt has no text")
                    .render(),
            ];
        }
        let _ = self.sessions.record_user_message(&text);
        self.active_turns.insert(parsed.session_id.clone());
        let answer = self.backend.prompt(text).await;
        self.active_turns.remove(&parsed.session_id);
        match answer {
            Ok(text) => {
                let _ = self.sessions.record_assistant_message(
                    Value::String(text.clone()),
                    None,
                    None,
                );
                vec![
                    chunk_notification(&parsed.session_id, &text).render(),
                    RpcResponse::ok(Some(id), json!({ "stopReason": "end_turn" })).render(),
                ]
            }
            Err(e) => vec![
                RpcResponse::err(Some(id), codes::BACKEND, format!("turn failed: {e}"))
                    .render(),
            ],
        }
    }

    async fn handle_cancel(&mut self, id: RpcId, params: Value) -> Vec<String> {
        if let Err(line) = self.initialized_or(&id) {
            return vec![line];
        }
        let parsed: CancelParams = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                return vec![
                    RpcResponse::err(
                        Some(id),
                        codes::INVALID_PARAMS,
                        format!("bad session/cancel params: {e}"),
                    )
                    .render(),
                ]
            }
        };
        if !self.live.contains(&parsed.session_id) {
            return vec![
                RpcResponse::err(Some(id), codes::UNKNOWN_SESSION, "unknown session")
                    .render(),
            ];
        }
        if self.active_turns.remove(&parsed.session_id) {
            self.backend.interrupt().await;
            let _ = self.sessions.record_assistant_message(
                Value::String("[cancelled]".into()),
                None,
                None,
            );
            vec![RpcResponse::ok(Some(id), json!({ "stopReason": "cancelled" })).render()]
        } else {
            vec![RpcResponse::ok(Some(id), json!({})).render()]
        }
    }
}

/// Run the adapter on stdio with the production Claude backend.
pub async fn run_claude_stdio(
    sessions_root: PathBuf,
    default_cwd: PathBuf,
) -> std::io::Result<()> {
    let mut agent = Agent::new(ClaudeBackend::new(), sessions_root, default_cwd);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        for reply in agent.handle_line(&line).await {
            out.write_all(reply.as_bytes()).await?;
            out.write_all(b"\n").await?;
        }
        out.flush().await?;
    }
    Ok(())
}

/// Scripted backend for tests: canned answers in order, records prompts.
pub struct ScriptBackend {
    pub seen: Vec<String>,
    pub replies: VecDeque<Result<String, String>>,
}

impl ScriptBackend {
    pub fn answering(text: &str) -> Self {
        let mut replies = VecDeque::new();
        replies.push_back(Ok(text.to_string()));
        Self {
            seen: Vec::new(),
            replies,
        }
    }
}

impl Backend for ScriptBackend {
    async fn prompt(&mut self, text: String) -> Result<String, String> {
        self.seen.push(text);
        self.replies.pop_front().unwrap_or(Ok(String::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContentBlock;
    use serde_json::json;

    fn agent() -> (tempfile::TempDir, Agent<ScriptBackend>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("sessions");
        let backend = ScriptBackend::answering("done-result");
        let agent = Agent::new(backend, root, PathBuf::from("/home/user/project"));
        (dir, agent)
    }

    fn rpc(id: i64, method: &str, params: Value) -> String {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
    }

    fn result_of(line: &str) -> Value {
        serde_json::from_str::<Value>(line).unwrap()["result"].clone()
    }

    fn error_of(line: &str) -> Value {
        serde_json::from_str::<Value>(line).unwrap()["error"].clone()
    }

    #[tokio::test]
    async fn initialize_then_full_turn() {
        let (_dir, mut agent) = agent();
        let out = agent
            .handle_line(&rpc(1, "initialize", json!({ "protocolVersion": 1 })))
            .await;
        assert_eq!(out.len(), 1);
        assert_eq!(result_of(&out[0])["protocolVersion"], 1);

        let out = agent
            .handle_line(&rpc(2, "session/new", json!({ "cwd": "/home/user/project" })))
            .await;
        let session_id = result_of(&out[0])["sessionId"].as_str().unwrap().to_string();

        let out = agent
            .handle_line(&rpc(
                3,
                "session/prompt",
                json!({ "sessionId": session_id, "prompt": [{ "type": "text", "text": "do it" }] }),
            ))
            .await;
        assert_eq!(out.len(), 2);
        let note: Value = serde_json::from_str(&out[0]).unwrap();
        assert_eq!(note["method"], "session/update");
        assert!(note["params"]["update"]["text"]
            .as_str()
            .unwrap()
            .contains("done-result"));
        assert_eq!(result_of(&out[1])["stopReason"], "end_turn");

        // The turn is recorded in the session store.
        let sessions = agent.sessions.list_sessions().unwrap();
        assert!(sessions.is_empty(), "index fills at end()");
        agent.sessions.end(0).unwrap();
        assert_eq!(agent.sessions.list_sessions().unwrap().len(), 1);
        assert_eq!(agent.backend.seen, vec!["do it".to_string()]);
    }

    #[tokio::test]
    async fn lifecycle_guards() {
        let (_dir, mut agent) = agent();
        // Session before initialize is rejected.
        let out = agent
            .handle_line(&rpc(1, "session/new", json!({})))
            .await;
        assert_eq!(error_of(&out[0])["code"], codes::NOT_INITIALIZED);
        // Unknown method and garbage input.
        agent
            .handle_line(&rpc(1, "initialize", json!({})))
            .await;
        let out = agent.handle_line(&rpc(2, "frobnicate", json!({}))).await;
        assert_eq!(error_of(&out[0])["code"], codes::METHOD_NOT_FOUND);
        let out = agent.handle_line("{oops").await;
        assert_eq!(error_of(&out[0])["code"], codes::PARSE);
        // Notifications (no id) get no answer.
        let out = agent
            .handle_line(r#"{"jsonrpc":"2.0","method":"session/update","params":{}}"#)
            .await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn prompt_guards_and_cancel() {
        let (_dir, mut agent) = agent();
        agent
            .handle_line(&rpc(1, "initialize", json!({})))
            .await;
        // Unknown session.
        let out = agent
            .handle_line(&rpc(
                2,
                "session/prompt",
                json!({ "sessionId": "nope", "prompt": [{ "type": "text", "text": "x" }] }),
            ))
            .await;
        assert_eq!(error_of(&out[0])["code"], codes::UNKNOWN_SESSION);
        // Empty prompt text.
        let out = agent
            .handle_line(&rpc(2, "session/new", json!({})))
            .await;
        let session_id = result_of(&out[0])["sessionId"].as_str().unwrap().to_string();
        let out = agent
            .handle_line(&rpc(
                3,
                "session/prompt",
                json!({ "sessionId": session_id, "prompt": [{ "type": "image" }] }),
            ))
            .await;
        assert_eq!(error_of(&out[0])["code"], codes::INVALID_PARAMS);
        // Idle cancel is a quiet ok.
        let out = agent
            .handle_line(&rpc(4, "session/cancel", json!({ "sessionId": session_id })))
            .await;
        assert!(result_of(&out[0]).as_object().unwrap().is_empty());
    }

    #[test]
    fn prompt_text_collects_text_blocks() {
        let blocks = vec![
            ContentBlock {
                block_type: "text".into(),
                text: Some("a".into()),
            },
            ContentBlock {
                block_type: "image".into(),
                text: None,
            },
            ContentBlock {
                block_type: "text".into(),
                text: Some("b".into()),
            },
        ];
        assert_eq!(prompt_text(&blocks), "a\nb");
    }

    #[test]
    fn response_renders_without_result_or_error_leak() {
        let line =
            RpcResponse::err(Some(RpcId::Number(1)), codes::BACKEND, "boom").render();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert!(v.get("result").is_none());
        assert_eq!(v["error"]["code"], codes::BACKEND);
    }
}
