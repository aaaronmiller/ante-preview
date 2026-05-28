//! Integration test for inter-agent communication via broker.
//! Tests T053: Two Ante instances discover each other and exchange messages.

use std::sync::Arc;
use tempfile::TempDir;
use std::time::Duration;

use agent_sdk::agents::{
    Broker, Transport, connect_to_broker,
};

#[tokio::test]
async fn test_broker_connects_and_counts_agents() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("test.sock");

    let broker = Arc::new(Broker::bind(Transport::unix(sock_path.clone()), true).await.unwrap());
    broker.start_and_wait().await;

    let (_h, _w, _ack) = connect_to_broker(&Transport::unix(sock_path.clone()), "agent-alpha").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(broker.connection_count().await, 1);

    broker.shutdown().await;
}

#[tokio::test]
async fn test_two_agents_can_connect() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("multi.sock");

    let broker = Arc::new(Broker::bind(Transport::unix(sock_path.clone()), true).await.unwrap());
    broker.start_and_wait().await;

    let (_h1, _w1, ack1) = connect_to_broker(&Transport::unix(sock_path.clone()), "agent-1").await.unwrap();
    let _ = ack1.await;
    let (_h2, _w2, ack2) = connect_to_broker(&Transport::unix(sock_path.clone()), "agent-2").await.unwrap();
    let _ = ack2.await;

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(broker.connection_count().await, 2);

    let agents = broker.connected_agents().await;
    assert!(agents.contains(&"agent-1".to_string()));
    assert!(agents.contains(&"agent-2".to_string()));

    broker.shutdown().await;
}

#[tokio::test]
async fn test_broker_shutdown_removes_socket() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("cleanup.sock");

    {
        let broker = Arc::new(Broker::bind(Transport::unix(sock_path.clone()), true).await.unwrap());
        broker.start_and_wait().await;
        assert!(sock_path.exists());
        broker.shutdown().await;
    }

    assert!(!sock_path.exists(), "socket should be cleaned up on shutdown");
}

#[tokio::test]
async fn test_broker_rejects_bad_registration() {
    let tmp = TempDir::new().unwrap();
    let sock_path = tmp.path().join("badreg.sock");

    let broker = Arc::new(Broker::bind(Transport::unix(sock_path.clone()), true).await.unwrap());
    broker.start_and_wait().await;

    use tokio::io::AsyncWriteExt;
    let stream = tokio::net::UnixStream::connect(&sock_path).await.unwrap();
    let (_, mut writer) = stream.into_split();
    writer.write_all(b"invalid json no newline").await.unwrap();
    writer.flush().await.unwrap();

    // Give broker time to process
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(broker.connection_count().await, 0);
    broker.shutdown().await;
}
