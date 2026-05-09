use std::cell::RefCell;
use std::sync::{Arc, Mutex};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentTaskStatus {
    Success,
    Failed,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct AgentTaskRequest {
    pub task_name: String,
    pub agent_id: String,
    pub agent_name: String,
    pub user_ip: Option<String>,
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

pub trait AgentTaskRuntime: Send + Sync {
    fn start_task(&self, request: AgentTaskRequest) -> Arc<AgentTaskHandle>;
}
