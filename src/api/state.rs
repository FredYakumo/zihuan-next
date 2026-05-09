use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_graph_engine::graph_io::NodeGraphDefinition;

use crate::service::AgentManager;

pub struct AppState {
    pub sessions: RwLock<HashMap<String, GraphSession>>,
    pub tasks: Mutex<TaskManager>,
    pub agent_manager: AgentManager,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            tasks: Mutex::new(TaskManager::new()),
            agent_manager: AgentManager::new(),
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
            graph: zihuan_graph_engine::graph_boundary::default_root_graph_definition(),
            dirty: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    NodeGraph,
    AgentService,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEntry {
    pub id: String,
    pub task_type: TaskType,
    pub graph_name: String,
    pub graph_session_id: String,
    pub file_path: Option<String>,
    pub is_workflow_set: bool,
    pub start_time: DateTime<Local>,
    pub is_running: bool,
    pub end_time: Option<DateTime<Local>>,
    pub duration_ms: Option<i64>,
    pub user_ip: Option<String>,
    pub status: TaskStatus,
    pub error_message: Option<String>,
    pub result_summary: Option<String>,
    pub log_path: Option<String>,
    pub can_rerun: bool,
    #[serde(skip, default)]
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
        let mut manager = Self { tasks: Vec::new() };
        if let Err(err) = manager.load_persisted_tasks() {
            log::warn!("Failed to load persisted task records: {}", err);
        }
        manager
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
        let can_rerun = file_path.is_some();
        self.add_task_with_type(
            TaskType::NodeGraph,
            graph_name,
            graph_session_id,
            file_path,
            is_workflow_set,
            user_ip,
            Some(stop_flag),
            can_rerun,
        )
    }

    pub fn add_agent_response_task(
        &mut self,
        agent_id: String,
        task_name: String,
        user_ip: Option<String>,
    ) -> String {
        self.add_task_with_type(
            TaskType::AgentService,
            task_name,
            agent_id,
            None,
            false,
            user_ip,
            None,
            false,
        )
    }

    fn add_task_with_type(
        &mut self,
        task_type: TaskType,
        graph_name: String,
        graph_session_id: String,
        file_path: Option<String>,
        is_workflow_set: bool,
        user_ip: Option<String>,
        stop_flag: Option<Arc<AtomicBool>>,
        can_rerun: bool,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let log_path = Self::task_log_path(&id).ok();
        self.tasks.push(TaskEntry {
            id: id.clone(),
            task_type,
            graph_name,
            graph_session_id,
            can_rerun,
            file_path,
            is_workflow_set,
            start_time: Local::now(),
            is_running: true,
            end_time: None,
            duration_ms: None,
            user_ip,
            status: TaskStatus::Running,
            error_message: None,
            result_summary: None,
            log_path,
            stop_flag,
        });
        self.persist_index();
        id
    }

    pub fn stop_task(&mut self, id: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if let Some(flag) = &task.stop_flag {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
                return true;
            }
            return false;
        }

        false
    }

    pub fn finish_task(
        &mut self,
        id: &str,
        status: TaskStatus,
        error_message: Option<String>,
        result_summary: Option<String>,
    ) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            let end_time = Local::now();
            task.is_running = false;
            task.end_time = Some(end_time);
            task.duration_ms = Some((end_time - task.start_time).num_milliseconds().max(0));
            task.status = status;
            task.error_message = error_message;
            task.result_summary = result_summary;
            task.stop_flag = None;
            self.persist_index();
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

        self.persist_index();
        before.saturating_sub(self.tasks.len())
    }

    pub fn delete_task(&mut self, id: &str) -> bool {
        let Some(index) = self.tasks.iter().position(|task| task.id == id) else {
            return false;
        };
        let task = self.tasks.remove(index);
        if let Some(flag) = task.stop_flag {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(path) = task.log_path {
            let _ = fs::remove_file(path);
        }
        self.persist_index();
        true
    }

    pub fn delete_tasks(&mut self, ids: &[String]) -> usize {
        let before = self.tasks.len();
        for id in ids {
            let _ = self.delete_task(id);
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
        Ok(dir
            .join(format!("{task_id}.jsonl"))
            .to_string_lossy()
            .to_string())
    }

    fn task_index_path() -> PathBuf {
        Path::new("logs").join("tasks").join("index.json")
    }

    fn persist_index(&self) {
        if let Err(err) = self.write_index() {
            log::warn!("Failed to persist task records: {}", err);
        }
    }

    fn write_index(&self) -> std::io::Result<()> {
        let path = Self::task_index_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.tasks).map_err(std::io::Error::other)?;
        fs::write(path, json)
    }

    fn load_persisted_tasks(&mut self) -> std::io::Result<()> {
        let index_path = Self::task_index_path();
        if index_path.exists() {
            let content = fs::read_to_string(&index_path)?;
            let mut tasks =
                serde_json::from_str::<Vec<TaskEntry>>(&content).map_err(std::io::Error::other)?;
            for task in &mut tasks {
                task.stop_flag = None;
                if task.is_running {
                    let end_time = Local::now();
                    task.is_running = false;
                    task.end_time.get_or_insert(end_time);
                    task.duration_ms = Some((end_time - task.start_time).num_milliseconds().max(0));
                    task.status = TaskStatus::Stopped;
                }
                if task.log_path.is_none() {
                    task.log_path = Self::task_log_path(&task.id).ok();
                }
            }
            self.tasks = tasks;
        }

        self.load_orphan_log_tasks()?;
        self.persist_index();
        Ok(())
    }

    fn load_orphan_log_tasks(&mut self) -> std::io::Result<()> {
        let dir = Path::new("logs").join("tasks");
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(task_id) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            if self.tasks.iter().any(|task| task.id == task_id) {
                continue;
            }

            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .map(DateTime::<Local>::from)
                .unwrap_or_else(|_| Local::now());
            let short_id = task_id.chars().take(8).collect::<String>();
            self.tasks.push(TaskEntry {
                id: task_id.to_string(),
                task_type: TaskType::NodeGraph,
                graph_name: format!("历史任务 {}", short_id),
                graph_session_id: String::new(),
                file_path: None,
                is_workflow_set: false,
                start_time: modified,
                is_running: false,
                end_time: Some(modified),
                duration_ms: Some(0),
                user_ip: None,
                status: TaskStatus::Success,
                error_message: None,
                result_summary: Some("从历史日志文件恢复的任务记录".to_string()),
                log_path: Some(path.to_string_lossy().to_string()),
                can_rerun: false,
                stop_flag: None,
            });
        }

        Ok(())
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
