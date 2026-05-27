//! Integration test for persistent memory store, query, and hooks.
//! Tests T038: Store, retrieve, rank, query memory; verify SessionStart context loading.

use tempfile::TempDir;
use agent_sdk::memory::MemoryStore;

#[tokio::test]
async fn test_memory_store_and_retrieve() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("memory.json");
    let mut store = MemoryStore::open(db_path.clone(), 1000).unwrap();

    store.add(
        "the config file uses JSON format".into(),
        "config, format".into(),
        "project-x".into(),
    ).unwrap();

    // search returns Vec directly, not Result
    let results = store.search("json format");
    assert!(!results.is_empty(), "should find memory by keyword");
    assert!(results[0].content.contains("JSON format"));
}

#[tokio::test]
async fn test_memory_persists_across_sessions() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("memory.json");

    // Session 1: store
    {
        let mut store = MemoryStore::open(db_path.clone(), 1000).unwrap();
        store.add(
            "session 1 learned something".into(),
            "learning".into(),
            "project-x".into(),
        ).unwrap();
    }

    // Session 2: retrieve
    {
        let store = MemoryStore::open(db_path.clone(), 1000).unwrap();
        let results = store.search("session 1");
        assert!(!results.is_empty(), "memory should persist across sessions");
    }
}

#[tokio::test]
async fn test_memory_scoped_by_project() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("memory.json");
    let mut store = MemoryStore::open(db_path.clone(), 1000).unwrap();

    store.add("project A setup".into(), "".into(), "project-a".into()).unwrap();
    store.add("project B setup".into(), "".into(), "project-b".into()).unwrap();

    // get_context returns Vec directly, not Result
    let ctx_a = store.get_context("project-a", 100);
    assert!(!ctx_a.is_empty(), "should find memory for project-a");
    assert!(ctx_a.iter().any(|e| e.content.contains("project A setup")));
    assert!(!ctx_a.iter().any(|e| e.content.contains("project B setup")));
}

#[tokio::test]
async fn test_memory_relevance_ranking() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("memory.json");
    let mut store = MemoryStore::open(db_path.clone(), 1000).unwrap();

    store.add("use serde for JSON serialization".into(), "serde, json".into(), "project-x".into()).unwrap();
    store.add("the API returns JSON responses".into(), "api, json".into(), "project-x".into()).unwrap();
    store.add("print hello world".into(), "hello".into(), "project-x".into()).unwrap();

    // search_ranked returns Vec<RankedEntry> directly
    let results = store.search_ranked("json serde");
    assert!(results.len() >= 2, "should find 2+ results");
    if results.len() >= 2 {
        assert!(
            results[0].score >= results[1].score,
            "first result should be most relevant"
        );
    }
}

#[tokio::test]
async fn test_memory_query_with_filters() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("memory.json");
    let mut store = MemoryStore::open(db_path.clone(), 1000).unwrap();

    store.add(
        "database connection string: localhost:5432".into(),
        "db, config".into(),
        "proj-a".into(),
    ).unwrap();
    store.add(
        "use InMemoryStore for testing".into(),
        "test, config".into(),
        "proj-b".into(),
    ).unwrap();

    // query returns Vec directly, not Result
    let results = store.query(None, Some("proj-a"), None, 10);
    assert_eq!(results.len(), 1, "should find only proj-a memory");
    assert!(results[0].content.contains("localhost"));

    // Query by tags
    let results = store.query(None, None, Some("test"), 10);
    assert_eq!(results.len(), 1, "should find memory with test tag");
    assert!(results[0].content.contains("InMemoryStore"));
}
