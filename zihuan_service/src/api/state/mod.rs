use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::broadcast;
use zihuan_service::RoleServiceManager;

use crate::setup_orchestrator::SetupProgressEvent;

mod chat;
mod graph;
mod task;

pub use chat::{RunningChatMessage, RunningChatToolCall};
pub use graph::{GraphSession, GraphTabInfo};
pub use task::{TaskEntry, TaskLogEntry, TaskManager, TaskStatus, TaskType};

pub struct AppState {
    pub sessions: RwLock<HashMap<String, GraphSession>>,
    pub tasks: Mutex<TaskManager>,
    pub role_service_manager: RoleServiceManager,
    pub setup_tasks: Mutex<HashMap<String, broadcast::Sender<SetupProgressEvent>>>,
    pub running_chat_messages: Mutex<HashMap<String, Arc<Mutex<RunningChatMessage>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            tasks: Mutex::new(TaskManager::new()),
            role_service_manager: RoleServiceManager::new(),
            setup_tasks: Mutex::new(HashMap::new()),
            running_chat_messages: Mutex::new(HashMap::new()),
        }
    }
}
