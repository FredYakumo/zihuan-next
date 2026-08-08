use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::task::JoinHandle;

use crate::agent::inference::LoadedInferenceAgent;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRuntimeStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentRuntimeInfo {
    pub agent_id: String,
    pub instance_id: Option<String>,
    pub status: AgentRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRuntimeState {
    pub instance_id: Option<String>,
    pub status: AgentRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self {
            instance_id: None,
            status: AgentRuntimeStatus::Stopped,
            started_at: None,
            last_error: None,
        }
    }
}

pub type OnFinishShared = Arc<Mutex<Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>>>;

pub struct AgentRuntimeEntry {
    pub loaded_agent: Option<Arc<LoadedInferenceAgent>>,
    pub state: AgentRuntimeState,
    pub task: Option<JoinHandle<()>>,
    pub on_finish: OnFinishShared,
}

impl Default for AgentRuntimeEntry {
    fn default() -> Self {
        Self {
            loaded_agent: None,
            state: AgentRuntimeState::default(),
            task: None,
            on_finish: Arc::new(Mutex::new(None)),
        }
    }
}

/// Trait for updating agent runtime state without depending on zihuan_service::AgentManager.
pub trait AgentStateManager: Send + Sync {
    fn update_agent_status(&self, agent_id: &str, status: AgentRuntimeStatus);
    fn update_agent_error(&self, agent_id: &str, error: Option<String>);
    fn update_agent_instance(&self, agent_id: &str, instance_id: Option<String>);
    fn update_state(&self, agent_id: &str, state: AgentRuntimeState) {
        self.update_agent_status(agent_id, state.status);
        self.update_agent_error(agent_id, state.last_error);
        self.update_agent_instance(agent_id, state.instance_id);
    }
}
