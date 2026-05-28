//! Inter-agent message broker with Unix domain socket and TCP transport.
//!
//! Provides a local or networked message bus for sub-agents to communicate:
//!   - `Broker` listens on a configurable transport, accepts connections
//!   - Messages are JSON-framed with `\n` delimiter
//!   - Supports broadcast, direct messaging, and pub/sub by topic
//!   - Automatic cleanup on Drop (removes socket file for UDS)

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedWriteHalf as TcpOwnedWriteHalf, ReadHalf as TcpReadHalf};
use tokio::net::unix::{OwnedWriteHalf as UnixOwnedWriteHalf, ReadHalf as UnixReadHalf};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};
use tokio::sync::{Mutex, broadcast, oneshot};

/// Operating system's ephemeral port range start (commonly 49152 on Linux).
/// We use this as a default port hint; the OS will assign the actual port.
const EPHEMERAL_PORT_START: u16 = 49152;

/// Format a UNIX timestamp as an RFC 3339-compatible UTC string
/// without requiring the `chrono` crate.
fn rfc3339_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let nanos = dur.subsec_nanos();
    // Produce "2025-12-31T23:59:60.123456789Z"
    // Use chrono-free formatting: compute date/time from secs
    let (y, m, d, hh, mm, ss) = seconds_to_ymdhms(secs);
    let subsec = if nanos == 0 {
        String::new()
    } else {
        format!(".{:09}", nanos)
    };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}{subsec}Z")
}

/// Convert seconds since epoch to (year, month, day, hour, minute, second).
fn seconds_to_ymdhms(secs: u64) -> (i32, u32, u32, u32, u32, u32) {
    // Days since epoch
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hh = (time_secs / 3600) as u32;
    let mm = ((time_secs % 3600) / 60) as u32;
    let ss = (time_secs % 60) as u32;

    // Civil date from days since 1970-01-01 (algorithm from Howard Hinnant)
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hh, mm, ss)
}

/// Transport type for the broker listener and connections.
///
/// Selects between Unix domain socket (local-only) or TCP (cross-network).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Transport {
    /// Unix domain socket at the given path.
    Unix {
        path: PathBuf,
    },
    /// TCP connection on the given address.
    Tcp {
        host: String,
        port: u16,
    },
}

impl Transport {
    /// Create a UDS transport at the given socket path.
    pub fn unix(path: impl Into<PathBuf>) -> Self {
        Transport::Unix { path: path.into() }
    }

    /// Create a TCP transport at the given host and port.
    /// Use port `0` to request an OS-assigned ephemeral port.
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        Transport::Tcp {
            host: host.into(),
            port,
        }
    }

    /// Return a human-readable address string for logging/display.
    pub fn address_string(&self) -> String {
        match self {
            Transport::Unix { path } => path.display().to_string(),
            Transport::Tcp { host, port } => format!("{host}:{port}"),
        }
    }

    /// Return the socket path if this is a Unix transport.
    pub fn unix_path(&self) -> Option<&PathBuf> {
        match self {
            Transport::Unix { path } => Some(path),
            Transport::Tcp { .. } => None,
        }
    }
}

/// Broker errors.
#[derive(Debug, Error)]
pub enum BrokerError {
    #[error("Failed to bind at {address}: {source}")]
    Bind {
        address: String,
        source: std::io::Error,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Agent {0} not connected")]
    AgentNotConnected(String),
    #[error("Broker shutting down")]
    Shutdown,
}

/// A message sent between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message ID.
    pub id: String,
    /// Sender agent ID.
    pub from: String,
    /// Recipient agent ID (or "*" for broadcast).
    pub to: String,
    /// Topic for pub/sub routing (empty for direct).
    #[serde(default)]
    pub topic: String,
    /// Message body.
    pub body: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Optional correlation ID for request/response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
}

/// Connection state for a connected agent.
struct AgentConnection {
    id: String,
    writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>,
}

/// Internal listener abstraction over UDS and TCP.
enum ListenerKind {
    Unix(UnixListener),
    Tcp(TcpListener),
}

/// Message broker for inter-agent communication.
///
/// Listens on a configurable transport (Unix domain socket or TCP) and
/// routes messages between connected agent processes. Use the
/// `Transport` enum to select between local-only UDS and cross-network
/// TCP communication.
pub struct Broker {
    listener: Option<ListenerKind>,
    transport: Transport,
    connections: Arc<Mutex<HashMap<String, AgentConnection>>>,
    shutdown: Arc<Mutex<bool>>,
    /// Broadcast channel for pub/sub topics.
    topic_tx: broadcast::Sender<(String, AgentMessage)>,
}

impl Broker {
    /// Create and bind a new broker on the given transport.
    ///
    /// For UDS transport, if `cleanup` is true, removes any existing socket
    /// file before binding. For TCP transport, the OS assigns the port if
    /// port 0 is given in the transport config.
    pub async fn bind(transport: Transport, cleanup: bool) -> Result<Self, BrokerError> {
        let listener = match &transport {
            Transport::Unix { path } => {
                if cleanup && path.exists() {
                    let _ = std::fs::remove_file(path);
                }
                let l = UnixListener::bind(path).map_err(|e| BrokerError::Bind {
                    address: transport.address_string(),
                    source: e,
                })?;
                ListenerKind::Unix(l)
            }
            Transport::Tcp { host, port } => {
                let addr: SocketAddr = format!("{host}:{port}")
                    .parse()
                    .map_err(|e| BrokerError::Bind {
                        address: transport.address_string(),
                        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                    })?;
                let l = TcpListener::bind(addr).await.map_err(|e| BrokerError::Bind {
                    address: transport.address_string(),
                    source: e,
                })?;
                ListenerKind::Tcp(l)
            }
        };

        let (topic_tx, _) = broadcast::channel(256);

        Ok(Broker {
            listener: Some(listener),
            transport,
            connections: Arc::new(Mutex::new(HashMap::new())),
            shutdown: Arc::new(Mutex::new(false)),
            topic_tx,
        })
    }

    /// Return the actual local address the broker is listening on.
    ///
    /// For TCP transports with port 0, this reveals the OS-assigned port.
    /// For UDS, returns the socket path as a `SocketAddr`-like string
    /// "unix:<path>" which callers can parse.
    pub fn local_address(&self) -> String {
        match &self.listener {
            Some(ListenerKind::Unix(_)) => format!("unix:{}", self.transport.address_string()),
            Some(ListenerKind::Tcp(l)) => match l.local_addr() {
                Ok(addr) => addr.to_string(),
                Err(_) => self.transport.address_string(),
            },
            None => self.transport.address_string(),
        }
    }

    /// Start accepting connections in the background.
    ///
    /// Spawns a task per accepted connection for message routing.
    /// Returns a receiver that signals when the accept loop has started.
    pub fn start(self: &Arc<Self>) -> oneshot::Receiver<()> {
        let this = self.clone();
        let (tx, rx) = oneshot::channel();
        tokio::spawn(async move {
            // Signal readiness before entering the accept loop
            let _ = tx.send(());
            this.accept_loop().await;
        });
        rx
    }

    /// Convenience: call start() and await readiness.
    pub async fn start_and_wait(self: &Arc<Self>) {
        let rx = self.start();
        let _ = rx.await;
    }

    /// Accept loop — runs forever until shutdown or error.
    async fn accept_loop(self: Arc<Self>) {
        let listener = match &self.listener {
            Some(l) => l,
            None => return,
        };

        loop {
            // Check shutdown flag
            if *self.shutdown.lock().await {
                break;
            }

            match self.accept_one(listener).await {
                Ok(stream) => {
                    let this = self.clone();
                    tokio::spawn(async move { this.handle_connection(stream).await });
                }
                Err(_) if *self.shutdown.lock().await => break,
                Err(e) => {
                    eprintln!("[broker] accept error: {e}");
                    // Brief pause to avoid tight loop on persistent error
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Accept a single connection, returning a generic stream handle.
    async fn accept_one(&self, listener: &ListenerKind) -> Result<Box<dyn AsyncReadWriteUnpinSend>, std::io::Error> {
        match listener {
            ListenerKind::Unix(l) => {
                let (stream, _) = l.accept().await?;
                Ok(Box::new(stream))
            }
            ListenerKind::Tcp(l) => {
                let (stream, _) = l.accept().await?;
                Ok(Box::new(stream))
            }
        }
    }

    /// Handle a single agent connection: register, route messages, cleanup.
    async fn handle_connection(self: Arc<Self>, stream: Box<dyn AsyncReadWriteUnpinSend>) {
        let (reader, writer) = tokio::io::split(stream);
        let writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>> =
            Arc::new(Mutex::new(Box::new(writer)));

        // Read the first message — must be a registration with agent ID
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();
        if buf_reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return; // Connection closed before registration
        }

        let register_msg: AgentMessage = match serde_json::from_str(line.trim()) {
            Ok(m) => m,
            Err(_) => return, // Bad registration
        };

        let agent_id = register_msg.from.clone();

        // Register the connection
        {
            let mut conns = self.connections.lock().await;
            conns.insert(
                agent_id.clone(),
                AgentConnection {
                    id: agent_id.clone(),
                    writer: writer.clone(),
                },
            );
        }

        // Send acknowledgment
        let ack = AgentMessage {
            id: format!("ack-{agent_id}"),
            from: "broker".into(),
            to: agent_id.clone(),
            topic: String::new(),
            body: "connected".into(),
            timestamp: rfc3339_now(),
            correlation_id: None,
        };
        {
            let mut w = writer.lock().await;
            let _ = w.write_all(serde_json::to_string(&ack).unwrap().as_bytes()).await;
            let _ = w.write_all(b"\n").await;
            let _ = w.flush().await;
        }

        // Forward registration to connected agents (broadcast)
        self.broadcast(&register_msg).await;

        // Now process incoming messages
        loop {
            line.clear();
            match buf_reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {}
                Err(_) => break,
            }

            let msg: AgentMessage = match serde_json::from_str(line.trim()) {
                Ok(m) => m,
                Err(_) => continue, // Malformed message — skip
            };

            // Check for disconnect message
            if msg.body == "__disconnect__" {
                break;
            }

            self.route(&msg).await;
        }

        // Cleanup on disconnect
        let mut conns = self.connections.lock().await;
        conns.remove(&agent_id);

        // Notify others of disconnect
        let disconnect_msg = AgentMessage {
            id: format!("disc-{agent_id}"),
            from: "broker".into(),
            to: "*".into(),
            topic: "system".into(),
            body: format!("agent disconnected: {agent_id}"),
            timestamp: rfc3339_now(),
            correlation_id: None,
        };
        drop(conns);
        self.broadcast(&disconnect_msg).await;
    }

    /// Route a message to its recipient.
    async fn route(&self, msg: &AgentMessage) {
        if msg.to == "*" || msg.to == "all" {
            self.broadcast(msg).await;
        } else if !msg.topic.is_empty() {
            // Publish to topic subscribers
            let _ = self.topic_tx.send((msg.topic.clone(), msg.clone()));
        } else {
            // Direct message
            self.send_direct(msg).await;
        }
    }

    /// Send a message directly to a specific agent.
    pub async fn send_direct(&self, msg: &AgentMessage) {
        let conns = self.connections.lock().await;
        let target = if msg.to == "*" || msg.to == "all" {
            return; // Use broadcast for wildcard
        } else {
            msg.to.clone()
        };

        if let Some(conn) = conns.get(&target) {
            let w = conn.writer.clone();
            let mut writer = w.lock().await;
            let payload = serde_json::to_string(msg).unwrap();
            let _ = writer.write_all(payload.as_bytes()).await;
            let _ = writer.write_all(b"\n").await;
            let _ = writer.flush().await;
        }
    }

    /// Broadcast a message to all connected agents.
    pub async fn broadcast(&self, msg: &AgentMessage) {
        let conns = self.connections.lock().await;
        let payload = serde_json::to_string(msg).unwrap();
        for conn in conns.values() {
            if conn.id != msg.from {
                let w = conn.writer.clone();
                let mut writer = w.lock().await;
                let _ = writer.write_all(payload.as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
            }
        }
    }

    /// Send a message from outside (e.g. from the agent loop).
    pub async fn send(&self, msg: AgentMessage) -> Result<(), BrokerError> {
        if *self.shutdown.lock().await {
            return Err(BrokerError::Shutdown);
        }
        self.route(&msg).await;
        Ok(())
    }

    /// Get list of connected agent IDs.
    pub async fn connected_agents(&self) -> Vec<String> {
        let conns = self.connections.lock().await;
        conns.keys().cloned().collect()
    }

    /// Get the transport configuration.
    pub fn transport(&self) -> &Transport {
        &self.transport
    }

    /// Get the path being listened on (only for UDS transport).
    pub fn socket_path(&self) -> Option<&PathBuf> {
        self.transport.unix_path()
    }

    /// Check if the broker is shutting down.
    pub async fn is_shutdown(&self) -> bool {
        *self.shutdown.lock().await
    }

    /// Number of connected agents.
    pub async fn connection_count(&self) -> usize {
        self.connections.lock().await.len()
    }

    /// Gracefully shut down the broker.
    pub async fn shutdown(&self) {
        *self.shutdown.lock().await = true;

        // Notify all connected agents
        let bye = AgentMessage {
            id: "broker-shutdown".into(),
            from: "broker".into(),
            to: "*".into(),
            topic: "system".into(),
            body: "broker shutting down".into(),
            timestamp: rfc3339_now(),
            correlation_id: None,
        };
        self.broadcast(&bye).await;

        // Clean up socket file (UDS only)
        if let Some(path) = self.transport.unix_path() {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

impl Drop for Broker {
    fn drop(&mut self) {
        // Best-effort cleanup of the socket file (UDS only)
        if let Some(path) = self.transport.unix_path() {
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

// Type alias used internally to erase the concrete stream type.
// Both UnixStream and TcpStream implement these traits.
trait AsyncReadWriteUnpinSend: AsyncRead + AsyncWrite + Unpin + Send {}
impl AsyncReadWriteUnpinSend for UnixStream {}
impl AsyncReadWriteUnpinSend for TcpStream {}

/// Helper to build a registration message for connecting to a broker.
pub fn register_msg(agent_id: &str) -> AgentMessage {
    AgentMessage {
        id: format!("reg-{agent_id}"),
        from: agent_id.into(),
        to: "broker".into(),
        topic: String::new(),
        body: "register".into(),
        timestamp: rfc3339_now(),
        correlation_id: None,
    }
}

/// Helper to build a direct message.
pub fn direct_msg(from: &str, to: &str, body: &str) -> AgentMessage {
    AgentMessage {
        id: format!("msg-{from}-{to}-{}", rfc3339_now()),
        from: from.into(),
        to: to.into(),
        topic: String::new(),
        body: body.into(),
        timestamp: rfc3339_now(),
        correlation_id: None,
    }
}

/// Connect to a broker via the given transport.
///
/// For UDS transports, connects via Unix domain socket at the given path.
/// For TCP transports, connects via TCP to the given host:port.
///
/// Returns (reader_task_handle, writer, ack_receiver) for bidirectional
/// communication. The `ack_rx` receiver will get the broker's ACK message
/// when registration is confirmed.
pub async fn connect_to_broker(
    transport: &Transport,
    agent_id: &str,
) -> Result<
    (
        tokio::task::JoinHandle<()>,
        Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>>,
        oneshot::Receiver<AgentMessage>,
    ),
    BrokerError,
> {
    // Connect via the appropriate transport
    let stream: Box<dyn AsyncReadWriteUnpinSend> = match transport {
        Transport::Unix { path } => {
            let s = UnixStream::connect(path).await?;
            Box::new(s)
        }
        Transport::Tcp { host, port } => {
            let addr: SocketAddr = format!("{host}:{port}")
                .parse()
                .map_err(|e| BrokerError::Bind {
                    address: transport.address_string(),
                    source: std::io::Error::new(std::io::ErrorKind::InvalidInput, e),
                })?;
            let s = TcpStream::connect(addr).await?;
            Box::new(s)
        }
    };

    let (reader, writer) = tokio::io::split(stream);
    let writer: Arc<Mutex<Box<dyn AsyncWrite + Unpin + Send>>> =
        Arc::new(Mutex::new(Box::new(writer)));

    // Channel to receive the ACK
    let (ack_tx, ack_rx) = oneshot::channel();
    let ack_tx = Arc::new(Mutex::new(Some(ack_tx)));

    // Send registration
    {
        let mut w = writer.lock().await;
        let reg = register_msg(agent_id);
        let payload = serde_json::to_string(&reg).unwrap();
        w.write_all(payload.as_bytes()).await?;
        w.write_all(b"\n").await?;
        w.flush().await?;
    }

    // Spawn a reader task to receive messages
    let w_clone = writer.clone();
    let handle = tokio::spawn(async move {
        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();
        loop {
            line.clear();
            match buf_reader.read_line(&mut line).await {
                Ok(0) => break,
                Ok(_) => {
                    if let Ok(msg) = serde_json::from_str::<AgentMessage>(line.trim()) {
                        let is_shutdown = msg.body == "broker shutting down";
                        // Forward ACK to caller if not already done
                        if msg.body == "connected" && msg.from == "broker" {
                            if let Some(tx) = ack_tx.lock().await.take() {
                                let _ = tx.send(msg);
                            }
                        }
                        if is_shutdown {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok((handle, w_clone, ack_rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UDS tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn uds_broker_binds_and_accepts_connection() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("test.sock");
        let transport = Transport::unix(sock_path.clone());

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        broker.start_and_wait().await;

        let transport = Transport::unix(sock_path.clone());
        let (_h1, _w, ack_rx) = connect_to_broker(&transport, "agent-1").await.unwrap();
        let _ = ack_rx.await;
        assert_eq!(broker.connection_count().await, 1);

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn uds_broker_routes_direct_message() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("direct.sock");
        let transport = Transport::unix(sock_path.clone());

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        broker.start_and_wait().await;

        let transport = Transport::unix(sock_path.clone());
        let (_h1, _w, ack_rx) = connect_to_broker(&transport, "agent-1").await.unwrap();
        let _ = ack_rx.await;
        assert_eq!(broker.connection_count().await, 1);

        let agents = broker.connected_agents().await;
        assert!(agents.contains(&"agent-1".to_string()));

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn uds_broker_cleanup_on_shutdown() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("cleanup.sock");
        let transport = Transport::unix(sock_path.clone());

        {
            let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
            broker.start_and_wait().await;
            assert!(sock_path.exists());
            broker.shutdown().await;
        }

        assert!(!sock_path.exists());
    }

    #[tokio::test]
    async fn uds_broker_multiple_agents() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("multi.sock");
        let transport = Transport::unix(sock_path.clone());

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        broker.start_and_wait().await;

        let mut _writers = Vec::new();
        for i in 1..=3 {
            let transport = Transport::unix(sock_path.clone());
            let (_h, w, ack_rx) =
                connect_to_broker(&transport, &format!("agent-{i}")).await.unwrap();
            _writers.push((_h, w));
            let _ = ack_rx.await;
        }

        assert_eq!(broker.connection_count().await, 3);

        let agents = broker.connected_agents().await;
        for i in 1..=3 {
            assert!(agents.contains(&format!("agent-{i}")));
        }

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn uds_broker_rejects_bad_registration() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("badreg.sock");
        let transport = Transport::unix(sock_path.clone());

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        broker.start_and_wait().await;

        // Connect without a proper registration message
        let transport = Transport::unix(sock_path.clone());
        let stream = match &transport {
            Transport::Unix { path } => UnixStream::connect(path).await.unwrap(),
            _ => unreachable!(),
        };
        let (_reader, mut writer) = stream.into_split();
        writer.write_all(b"not valid json\n").await.unwrap();
        writer.flush().await.unwrap();
        drop(writer);

        assert_eq!(broker.connection_count().await, 0);

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn uds_send_direct_message() {
        let tmp = tempfile::tempdir().unwrap();
        let sock_path = tmp.path().join("send.sock");
        let transport = Transport::unix(sock_path.clone());

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        broker.start_and_wait().await;

        let transport = Transport::unix(sock_path.clone());
        let (_h1, _w, ack_rx) = connect_to_broker(&transport, "agent-1").await.unwrap();
        let _ = ack_rx.await;

        let msg = direct_msg("broker", "agent-1", "hello from broker");
        broker.send(msg).await.unwrap();

        assert_eq!(broker.connection_count().await, 1);

        broker.shutdown().await;
    }

    // ── TCP tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn tcp_broker_binds_and_accepts_connection() {
        let transport = Transport::tcp("127.0.0.1", 0);

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        let addr = broker.local_address();
        broker.start_and_wait().await;

        // Parse the actual port from local_address and connect via TCP
        let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();
        let transport = Transport::tcp("127.0.0.1", port);
        let (_h1, _w, ack_rx) = connect_to_broker(&transport, "agent-1").await.unwrap();
        let _ = ack_rx.await;
        assert_eq!(broker.connection_count().await, 1);

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn tcp_broker_multiple_agents() {
        let transport = Transport::tcp("127.0.0.1", 0);

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        let addr = broker.local_address();
        broker.start_and_wait().await;

        let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();
        let mut _writers = Vec::new();
        for i in 1..=3 {
            let transport = Transport::tcp("127.0.0.1", port);
            let (_h, w, ack_rx) =
                connect_to_broker(&transport, &format!("agent-{i}")).await.unwrap();
            _writers.push((_h, w));
            let _ = ack_rx.await;
        }

        assert_eq!(broker.connection_count().await, 3);
        let agents = broker.connected_agents().await;
        for i in 1..=3 {
            assert!(agents.contains(&format!("agent-{i}")));
        }

        broker.shutdown().await;
    }

    #[tokio::test]
    async fn tcp_direct_message() {
        let transport = Transport::tcp("127.0.0.1", 0);

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        let addr = broker.local_address();
        broker.start_and_wait().await;

        let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();
        let transport = Transport::tcp("127.0.0.1", port);
        let (_h1, _w, ack_rx) = connect_to_broker(&transport, "agent-1").await.unwrap();
        let _ = ack_rx.await;

        // Send a message from the broker to agent-1
        let msg = direct_msg("broker", "agent-1", "hello from TCP broker");
        broker.send(msg).await.unwrap();

        assert_eq!(broker.connection_count().await, 1);
        broker.shutdown().await;
    }

    #[tokio::test]
    async fn tcp_broker_rejects_bad_registration() {
        let transport = Transport::tcp("127.0.0.1", 0);

        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        let addr = broker.local_address();
        broker.start_and_wait().await;

        let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();

        // Connect without a proper registration message
        let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        let (_, mut writer) = stream.into_split();
        writer.write_all(b"not valid json\n").await.unwrap();
        writer.flush().await.unwrap();
        drop(writer);

        assert_eq!(broker.connection_count().await, 0);
        broker.shutdown().await;
    }

    #[tokio::test]
    async fn transport_serialization_roundtrip() {
        let unix = Transport::unix("/tmp/test.sock");
        let json = serde_json::to_string(&unix).unwrap();
        let back: Transport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.address_string(), "/tmp/test.sock");

        let tcp = Transport::tcp("0.0.0.0", 8080);
        let json = serde_json::to_string(&tcp).unwrap();
        let back: Transport = serde_json::from_str(&json).unwrap();
        assert_eq!(back.address_string(), "0.0.0.0:8080");
    }

    #[tokio::test]
    async fn tcp_broker_local_address_port_zero() {
        let transport = Transport::tcp("127.0.0.1", 0);
        let broker = Arc::new(Broker::bind(transport, true).await.unwrap());
        let addr = broker.local_address();

        // Should contain the actual port, not 0
        assert_ne!(addr, "127.0.0.1:0");
        let port: u16 = addr.rsplit(':').next().unwrap().parse().unwrap();
        assert!(port > 0);
        assert!(port <= 65535);

        broker.shutdown().await;
    }
}
