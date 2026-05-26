use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};
use serde::Serialize;

thread_local! {
    static CURRENT_TASK_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn scope_task_id<T>(task_id: impl Into<String>, f: impl FnOnce() -> T) -> T {
    let task_id = task_id.into();
    CURRENT_TASK_ID.with(|cell| {
        let previous = cell.replace(Some(task_id));
        let result = f();
        cell.replace(previous);
        result
    })
}

pub fn current_task_id() -> Option<String> {
    CURRENT_TASK_ID.with(|cell| cell.borrow().clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Success,
    Failed,
    Stopped,
    Running,
}

#[derive(Debug, Clone)]
pub struct AgentTaskRequest {
    pub task_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub user_ip: Option<String>,
    pub owner_id: Option<String>,
    pub task_db_connection_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AgentTaskResult {
    pub status: Option<AgentTaskStatus>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
}

pub struct AgentTaskHandle {
    pub task_id: String,
    finish: Mutex<Option<Box<dyn FnOnce(AgentTaskResult) + Send + 'static>>>,
}

impl AgentTaskHandle {
    pub fn new(
        task_id: String,
        finish: impl FnOnce(AgentTaskResult) + Send + 'static,
    ) -> Arc<Self> {
        Arc::new(Self {
            task_id,
            finish: Mutex::new(Some(Box::new(finish))),
        })
    }

    pub fn finish(self: &Arc<Self>, result: AgentTaskResult) {
        if let Some(callback) = self.finish.lock().unwrap().take() {
            callback(result);
        }
    }
}

/// Read-only snapshot of a background task's state.
///
/// Returned by [`AgentTaskRuntime::query_task`] and
/// [`AgentTaskRuntime::list_tasks`]. The `progress` field accumulates
/// intermediate messages produced while the task is running.
#[derive(Debug, Clone, Serialize)]
pub struct AgentTaskInfo {
    pub task_id: String,
    pub task_name: String,
    pub owner_id: Option<String>,
    pub agent_id: String,
    pub status: AgentTaskStatus,
    pub created_at: DateTime<Local>,
    pub finished_at: Option<DateTime<Local>>,
    pub progress: Vec<String>,
    pub result_summary: Option<String>,
    pub error_message: Option<String>,
}

pub trait AgentTaskRuntime: Send + Sync {
    fn start_task(&self, request: AgentTaskRequest) -> Arc<AgentTaskHandle>;

    /// Spawn a runner function as a background task managed by this runtime.
    ///
    /// The runtime creates a task record, spawns the runner via
    /// `tokio::spawn`, and returns a handle. The runner is responsible for
    /// calling [`AgentTaskHandle::finish`] when done.
    fn spawn_task(
        &self,
        request: AgentTaskRequest,
        runner: Box<dyn FnOnce() + Send + 'static>,
    ) -> Arc<AgentTaskHandle>;

    /// Look up a task by id.
    fn query_task(&self, task_id: &str) -> Option<AgentTaskInfo>;

    /// List all tasks belonging to `owner_id`.
    fn list_tasks(&self, owner_id: &str) -> Vec<AgentTaskInfo>;

    /// Append a progress message to a running task.
    fn append_task_progress(&self, task_id: &str, message: String);

    /// Request cancellation of a running task. Returns `true` if the task
    /// was found and a stop signal was set, `false` if the task does not
    /// exist or has already finished.
    fn cancel_task(&self, task_id: &str) -> bool;
}
