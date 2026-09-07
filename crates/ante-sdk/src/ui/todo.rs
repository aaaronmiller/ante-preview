//! Persistent todo list manager.
//!
//! Backed by `todo.json` in the ante directory. Supports add,
//! complete, list, and clear operations. Survives session restarts.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TodoError {
    #[error("Failed to read todos: {0}")]
    Read(std::io::Error),

    #[error("Failed to write todos: {0}")]
    Write(std::io::Error),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Todo #{0} not found")]
    NotFound(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: usize,
    pub text: String,
    pub done: bool,
}

/// Manages a persistent todo list.
pub struct TodoList {
    items: Vec<TodoItem>,
    path: PathBuf,
    next_id: usize,
}

impl TodoList {
    /// Open or create the todo list at the given path.
    pub fn open(path: PathBuf) -> Result<Self, TodoError> {
        let (items, next_id) = if path.exists() {
            let content = fs::read_to_string(&path).map_err(TodoError::Read)?;
            let items: Vec<TodoItem> = serde_json::from_str(&content).map_err(TodoError::Serde)?;
            let next_id = items.iter().map(|t| t.id).max().unwrap_or(0) + 1;
            (items, next_id)
        } else {
            (Vec::new(), 1)
        };

        Ok(TodoList { items, path, next_id })
    }

    /// Add a new todo.
    pub fn add(&mut self, text: &str) -> Result<TodoItem, TodoError> {
        let item = TodoItem {
            id: self.next_id,
            text: text.to_string(),
            done: false,
        };
        self.next_id += 1;
        self.items.push(item.clone());
        self.save()?;
        Ok(item)
    }

    /// Mark a todo as complete.
    pub fn complete(&mut self, id: usize) -> Result<TodoItem, TodoError> {
        let item = self
            .items
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(TodoError::NotFound(id))?;
        item.done = true;
        let result = item.clone();
        self.save()?;
        Ok(result)
    }

    /// List all todos.
    pub fn list(&self) -> &[TodoItem] {
        &self.items
    }

    /// Get incomplete todos only.
    pub fn pending(&self) -> Vec<&TodoItem> {
        self.items.iter().filter(|t| !t.done).collect()
    }

    /// Remove all completed todos.
    pub fn clear_done(&mut self) -> Result<(), TodoError> {
        self.items.retain(|t| !t.done);
        self.save()?;
        Ok(())
    }

    /// Delete a todo by id.
    pub fn delete(&mut self, id: usize) -> Result<(), TodoError> {
        let pos = self
            .items
            .iter()
            .position(|t| t.id == id)
            .ok_or(TodoError::NotFound(id))?;
        self.items.remove(pos);
        self.save()?;
        Ok(())
    }

    fn save(&self) -> Result<(), TodoError> {
        let json = serde_json::to_string_pretty(&self.items).map_err(TodoError::Serde)?;
        fs::write(&self.path, &json).map_err(TodoError::Write)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_list_todos() {
        let tmp = tempfile::tempdir().unwrap();
        let mut todos = TodoList::open(tmp.path().join("todos.json")).unwrap();

        todos.add("Write tests").unwrap();
        todos.add("Implement feature").unwrap();

        assert_eq!(todos.list().len(), 2);
        assert_eq!(todos.pending().len(), 2);
    }

    #[test]
    fn complete_todo() {
        let tmp = tempfile::tempdir().unwrap();
        let mut todos = TodoList::open(tmp.path().join("todos.json")).unwrap();

        todos.add("Task 1").unwrap();
        let item = todos.add("Task 2").unwrap();
        todos.complete(item.id).unwrap();

        assert!(todos.list()[1].done);
        assert_eq!(todos.pending().len(), 1);
    }

    #[test]
    fn clear_done_removes_completed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut todos = TodoList::open(tmp.path().join("todos.json")).unwrap();

        let t1 = todos.add("Task 1").unwrap();
        todos.add("Task 2").unwrap();
        todos.complete(t1.id).unwrap();
        todos.clear_done().unwrap();

        assert_eq!(todos.list().len(), 1);
        assert_eq!(todos.list()[0].text, "Task 2");
    }

    #[test]
    fn persist_survives_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("todos.json");

        {
            let mut todos = TodoList::open(path.clone()).unwrap();
            todos.add("Persistent task").unwrap();
        }

        {
            let todos = TodoList::open(path).unwrap();
            assert_eq!(todos.list().len(), 1);
        }
    }

    #[test]
    fn delete_removes_todo() {
        let tmp = tempfile::tempdir().unwrap();
        let mut todos = TodoList::open(tmp.path().join("todos.json")).unwrap();

        let t = todos.add("Delete me").unwrap();
        assert_eq!(todos.list().len(), 1);
        todos.delete(t.id).unwrap();
        assert_eq!(todos.list().len(), 0);
    }
}
