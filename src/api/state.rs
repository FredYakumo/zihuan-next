use std::collections::HashMap;
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
    pub start_time: DateTime<Local>,
    pub is_running: bool,
    pub end_time: Option<DateTime<Local>>,
    #[serde(skip)]
    pub stop_flag: Option<Arc<AtomicBool>>,
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
        stop_flag: Arc<AtomicBool>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        self.tasks.push(TaskEntry {
            id: id.clone(),
            graph_name,
            graph_session_id,
            start_time: Local::now(),
            is_running: true,
            end_time: None,
            stop_flag: Some(stop_flag),
        });
        id
    }

    pub fn stop_task(&mut self, id: &str) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if let Some(flag) = &task.stop_flag {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    pub fn finish_task(&mut self, id: &str, success: bool) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.is_running = false;
            task.end_time = Some(Local::now());
            let _ = success;
        }
    }

    pub fn list(&self) -> Vec<&TaskEntry> {
        let mut entries: Vec<&TaskEntry> = self.tasks.iter().collect();
        // Newest first
        entries.sort_by(|a, b| b.start_time.cmp(&a.start_time));
        entries
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
