use chrono::{DateTime, Local};
use lazy_static::lazy_static;
use slint::{ModelRc, VecModel};
use std::sync::{Arc, Mutex};

use crate::ui::graph_window::{NodeGraphWindow, TaskEntryVm};

lazy_static! {
    pub static ref TASK_MANAGER: Arc<Mutex<TaskManager>> =
        Arc::new(Mutex::new(TaskManager::new()));
}

#[derive(Clone)]
pub struct TaskEntry {
    pub id: u64,
    pub graph_name: String,
    pub start_time: DateTime<Local>,
    pub is_running: bool,
    pub end_time: Option<DateTime<Local>>,
}

pub struct TaskManager {
    pub tasks: Vec<TaskEntry>,
    next_id: u64,
}

impl TaskManager {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            next_id: 1,
        }
    }

    /// Register a new running task; returns its ID.
    pub fn add_task(&mut self, graph_name: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(TaskEntry {
            id,
            graph_name: graph_name.into(),
            start_time: Local::now(),
            is_running: true,
            end_time: None,
        });
        id
    }

    /// Mark a task as finished.
    pub fn finish_task(&mut self, id: u64) {
        if let Some(entry) = self.tasks.iter_mut().find(|e| e.id == id) {
            entry.is_running = false;
            entry.end_time = Some(Local::now());
        }
    }

    /// Number of currently running tasks.
    pub fn running_count(&self) -> usize {
        self.tasks.iter().filter(|e| e.is_running).count()
    }

    /// Convert tasks to Slint view-model entries (newest first).
    pub fn to_vm_entries(&self) -> Vec<TaskEntryVm> {
        self.tasks
            .iter()
            .rev()
            .map(|e| TaskEntryVm {
                task_id: e.id.to_string().into(),
                graph_name: e.graph_name.clone().into(),
                start_time: e.start_time.format("%H:%M:%S").to_string().into(),
                is_running: e.is_running,
                end_time: e
                    .end_time
                    .map(|t| t.format("%H:%M:%S").to_string())
                    .unwrap_or_default()
                    .into(),
            })
            .collect()
    }
}

/// Push the current task list and running-count to the UI.
/// Must be called from the Slint event-loop thread.
pub fn push_task_list_to_ui(ui: &NodeGraphWindow) {
    let manager = TASK_MANAGER.lock().unwrap();
    let entries = manager.to_vm_entries();
    let running = manager.running_count() as i32;
    drop(manager);

    ui.set_task_entries(ModelRc::new(VecModel::from(entries)));
    ui.set_running_task_count(running);
}
