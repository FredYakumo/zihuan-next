use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_node::graph_io::NodeGraphDefinition;


pub struct AppState {
    pub sessions: RwLock<HashMap<String, GraphSession>>,
    pub tasks: Mutex<TaskManager>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            tasks: Mutex::new(TaskManager::new()),
        }
    }
}


pub struct GraphSession {
    pub id: String,
    /// Optional filesystem path for save/load
    pub file_path: Option<String>,
    pub graph: NodeGraphDefinition,
    pub dirty: bool,
}

impl GraphSession {
    pub fn new(id: String, graph: NodeGraphDefinition, file_path: Option<String>) -> Self {
        Self {
            id,
            file_path,
            graph,
            dirty: false,
        }
    }

    pub fn new_empty() -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            file_path: None,
            graph: NodeGraphDefinition::default(),
            dirty: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskEntry {
    pub id: String,
    pub graph_name: String,
    pub graph_session_id: String,
    pub file_path: Option<String>,
    pub is_workflow_set: bool,
    pub start_time: DateTime<Local>,
    pub is_running: bool,
    pub end_time: Option<DateTime<Local>>,
    pub user_ip: Option<String>,
    pub status: TaskStatus,
    pub error_message: Option<String>,
    pub log_path: Option<String>,
    pub can_rerun: bool,
    #[serde(skip)]
    pub stop_flag: Option<Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Success,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

pub struct TaskManager {
    tasks: Vec<TaskEntry>,
}

impl TaskManager {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn add_task(
        &mut self,
        graph_name: String,
        graph_session_id: String,
        file_path: Option<String>,
        is_workflow_set: bool,
        user_ip: Option<String>,
        stop_flag: Arc<AtomicBool>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let log_path = Self::task_log_path(&id).ok();
        self.tasks.push(TaskEntry {
            id: id.clone(),
            graph_name,
            graph_session_id,
            can_rerun: file_path.is_some(),
            file_path,
            is_workflow_set,
            start_time: Local::now(),
            is_running: true,
            end_time: None,
            user_ip,
            status: TaskStatus::Running,
            error_message: None,
            log_path,
            stop_flag: Some(stop_flag),
        });
        id
    }

    pub fn stop_task(&mut self, id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if let Some(flag) = &task.stop_flag {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            return true;
        }

        false
    }

    pub fn finish_task(&mut self, id: &str, status: TaskStatus, error_message: Option<String>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.is_running = false;
            task.end_time = Some(Local::now());
            task.status = status;
            task.error_message = error_message;
            task.stop_flag = None;
        }
    }

    pub fn list(&self) -> Vec<TaskEntry> {
        let mut entries = self.tasks.clone();
        // Newest first
        entries.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        entries
    }

    pub fn get(&self, id: &str) -> Option<&TaskEntry> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn clear_non_running(&mut self) -> usize {
        let before = self.tasks.len();
        let removed_paths: Vec<PathBuf> = self
            .tasks
            .iter()
            .filter(|task| !task.is_running)
            .filter_map(|task| task.log_path.as_ref().map(PathBuf::from))
            .collect();
        self.tasks.retain(|task| task.is_running);

        for path in removed_paths {
            let _ = fs::remove_file(path);
        }

        before.saturating_sub(self.tasks.len())
    }

    pub fn append_task_log(&self, task_id: &str, entry: &TaskLogEntry) -> std::io::Result<()> {
        let Some(task) = self.get(task_id) else {
            return Ok(());
        };
        let Some(path) = task.log_path.as_ref() else {
            return Ok(());
        };

        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        serde_json::to_writer(&mut file, entry)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    pub fn read_task_logs(&self, task_id: &str) -> std::io::Result<Vec<TaskLogEntry>> {
        let Some(task) = self.get(task_id) else {
            return Ok(Vec::new());
        };
        let Some(path) = task.log_path.as_ref() else {
            return Ok(Vec::new());
        };

        let file = match OpenOptions::new().read(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };

        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<TaskLogEntry>(&line) {
                entries.push(entry);
            }
        }
        Ok(entries)
    }

    fn task_log_path(task_id: &str) -> std::io::Result<String> {
        let dir = Path::new("logs").join("tasks");
        fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{task_id}.jsonl")).to_string_lossy().to_string())
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphTabInfo {
    pub id: String,
    pub name: String,
    pub file_path: Option<String>,
    pub dirty: bool,
    pub node_count: usize,
    pub edge_count: usize,
}

#[cfg(test)]
mod tests {
    use super::{TaskLogEntry, TaskManager, TaskStatus};
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn clears_only_non_running_tasks() {
        let mut manager = TaskManager::new();
        let running = manager.add_task(
            "running".to_string(),
            "session-running".to_string(),
            Some("workflow_set/running.json".to_string()),
            true,
            Some("127.0.0.1".to_string()),
            Arc::new(AtomicBool::new(false)),
        );
        let finished = manager.add_task(
            "finished".to_string(),
            "session-finished".to_string(),
            Some("saved/finished.json".to_string()),
            false,
            Some("127.0.0.2".to_string()),
            Arc::new(AtomicBool::new(false)),
        );
        manager.finish_task(&finished, TaskStatus::Success, None);

        let removed = manager.clear_non_running();

        assert_eq!(removed, 1);
        let tasks = manager.list();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, running);
    }

    #[test]
    fn persists_and_reads_task_logs() {
        let mut manager = TaskManager::new();
        let task_id = manager.add_task(
            "graph".to_string(),
            "session-1".to_string(),
            Some("saved/graph.json".to_string()),
            false,
            Some("127.0.0.1".to_string()),
            Arc::new(AtomicBool::new(false)),
        );
        manager
            .append_task_log(
                &task_id,
                &TaskLogEntry {
                    timestamp: "10:00:00.000".to_string(),
                    level: "INFO".to_string(),
                    message: "hello".to_string(),
                },
            )
            .unwrap();

        let logs = manager.read_task_logs(&task_id).unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "hello");

        manager.finish_task(&task_id, TaskStatus::Success, None);
        manager.clear_non_running();
    }
}
