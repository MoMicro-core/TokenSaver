use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::Path;

const TASKS_FILE: &str = ".tokensaver/tasks.jsonl";

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Active,
    Completed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Task {
    pub id: String,
    pub status: TaskStatus,
    pub description: String,
    pub prompt: String,
    pub timestamp: String,
}

/// Appends a new active task entry.
pub fn add(repo_root: &Path, description: &str, prompt: &str) -> Result<String> {
    let id = super::new_id();
    let task = Task {
        id: id.clone(),
        status: TaskStatus::Active,
        description: description.to_string(),
        prompt: prompt.to_string(),
        timestamp: super::changelog::current_timestamp(),
    };
    append_line(repo_root, &task)?;
    Ok(id)
}

/// Marks a task as completed by appending an updated entry (last write per id wins on load).
pub fn complete(repo_root: &Path, id: &str) -> Result<()> {
    let tasks = load_all(repo_root)?;
    let task = tasks
        .iter()
        .find(|t| t.id == id)
        .ok_or_else(|| anyhow::anyhow!("task '{id}' not found"))?;
    let completed = Task { status: TaskStatus::Completed, ..task.clone() };
    append_line(repo_root, &completed)
}

/// Returns all tasks, deduplicated — only the latest entry per id is kept.
pub fn load_all(repo_root: &Path) -> Result<Vec<Task>> {
    let path = repo_root.join(TASKS_FILE);
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    // Append-log pattern: last write per id wins.
    let mut by_id: std::collections::HashMap<String, Task> = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(task) = serde_json::from_str::<Task>(line) {
            by_id.insert(task.id.clone(), task);
        }
    }

    let mut tasks: Vec<Task> = by_id.into_values().collect();
    tasks.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(tasks)
}

pub fn load_active(repo_root: &Path) -> Result<Vec<Task>> {
    Ok(load_all(repo_root)?
        .into_iter()
        .filter(|t| t.status == TaskStatus::Active)
        .collect())
}

fn append_line(repo_root: &Path, task: &Task) -> Result<()> {
    let path = repo_root.join(TASKS_FILE);
    let line = serde_json::to_string(task).context("failed to serialize task")?;

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{line}")
        .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn init(dir: &std::path::Path) {
        std::fs::create_dir(dir.join(".tokensaver")).unwrap();
    }

    #[test]
    fn add_and_load() {
        let dir = tempdir().unwrap();
        init(dir.path());
        add(dir.path(), "Fix auth redirect", "fix login").unwrap();
        let tasks = load_all(dir.path()).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "Fix auth redirect");
        assert_eq!(tasks[0].status, TaskStatus::Active);
    }

    #[test]
    fn complete_marks_task() {
        let dir = tempdir().unwrap();
        init(dir.path());
        let id = add(dir.path(), "Fix auth", "fix").unwrap();
        complete(dir.path(), &id).unwrap();
        let tasks = load_all(dir.path()).unwrap();
        assert_eq!(tasks[0].status, TaskStatus::Completed);
    }

    #[test]
    fn load_active_filters_completed() {
        let dir = tempdir().unwrap();
        init(dir.path());
        let id = add(dir.path(), "Task A", "a").unwrap();
        add(dir.path(), "Task B", "b").unwrap();
        complete(dir.path(), &id).unwrap();
        let active = load_active(dir.path()).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].description, "Task B");
    }
}
