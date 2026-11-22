use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use zihuan_service::RoleServiceManager;

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
    pub running_chat_messages: Mutex<HashMap<String, Arc<Mutex<RunningChatMessage>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            tasks: Mutex::new(TaskManager::new()),
            role_service_manager: RoleServiceManager::new(),
            running_chat_messages: Mutex::new(HashMap::new()),
        }
    }
}
