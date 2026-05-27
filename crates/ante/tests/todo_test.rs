//! Integration test for persistent todo list.
//! Tests T043: Create todos, verify persistence across sessions.

use agent_sdk::ui::TodoList;
use tempfile::TempDir;

#[tokio::test]
async fn test_todo_create_and_list() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("todos.json");

    let mut todos = TodoList::open(file_path.clone()).unwrap();
    let item = todos.add("Buy milk").unwrap();
    assert_eq!(item.id, 1);

    let list = todos.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].text, "Buy milk");
    assert!(!list[0].done);
}

#[tokio::test]
async fn test_todo_complete_and_pending() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("todos.json");

    let mut todos = TodoList::open(file_path.clone()).unwrap();
    todos.add("Task A").unwrap();
    todos.add("Task B").unwrap();

    todos.complete(1).unwrap();

    let pending = todos.pending();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, "Task B");
}

#[tokio::test]
async fn test_todo_clear_done() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("todos.json");

    let mut todos = TodoList::open(file_path.clone()).unwrap();
    todos.add("Do this").unwrap();
    todos.add("Do that").unwrap();
    todos.complete(1).unwrap();

    todos.clear_done().unwrap();

    let all = todos.list();
    assert_eq!(all.len(), 1, "completed todo should be removed");
    assert_eq!(all[0].text, "Do that");
}

#[tokio::test]
async fn test_todo_persists_across_sessions() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("todos.json");

    // Session 1
    {
        let mut todos = TodoList::open(file_path.clone()).unwrap();
        todos.add("Remember this").unwrap();
    }

    // Session 2
    {
        let todos = TodoList::open(file_path.clone()).unwrap();
        let list = todos.list();
        assert_eq!(list.len(), 1, "todos should persist across sessions");
        assert_eq!(list[0].text, "Remember this");
    }
}

#[tokio::test]
async fn test_todo_delete() {
    let tmp = TempDir::new().unwrap();
    let file_path = tmp.path().join("todos.json");

    let mut todos = TodoList::open(file_path.clone()).unwrap();
    todos.add("Delete me").unwrap();
    todos.add("Keep me").unwrap();

    todos.delete(1).unwrap();

    let list = todos.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].text, "Keep me");
}
