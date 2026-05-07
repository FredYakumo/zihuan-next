pub mod http_stream_agent;
pub mod inference;
pub mod qq_chat_agent;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::error;
use serde::Serialize;
use storage_handler::{load_connections, ConnectionConfig};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zihuan_core::error::Result;
use zihuan_core::llm::OpenAIMessage;
use zihuan_llm::system_config::{load_agents, AgentConfig, AgentType};

use self::inference::LoadedInferenceAgent;

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
    pub status: AgentRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentRuntimeState {
    pub status: AgentRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for AgentRuntimeState {
    fn default() -> Self {
        Self {
            status: AgentRuntimeStatus::Stopped,
            started_at: None,
            last_error: None,
        }
    }
}

pub(super) type OnFinishShared =
    Arc<Mutex<Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>>>;

pub(super) struct AgentRuntimeEntry {
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

#[derive(Clone, Default)]
pub struct AgentManager {
    pub(super) inner: Arc<Mutex<HashMap<String, AgentRuntimeEntry>>>,
}

impl AgentManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn runtime_info(&self, agent_id: &str) -> AgentRuntimeInfo {
        let state = self
            .inner
            .lock()
            .unwrap()
            .get(agent_id)
            .map(|entry| entry.state.clone())
            .unwrap_or_default();
        AgentRuntimeInfo {
            agent_id: agent_id.to_string(),
            status: state.status,
            started_at: state.started_at,
            last_error: state.last_error,
        }
    }

    pub fn running_agent(&self, agent_id: &str) -> Option<Arc<LoadedInferenceAgent>> {
        let guard = self.inner.lock().unwrap();
        let entry = guard.get(agent_id)?;
        if entry.state.status != AgentRuntimeStatus::Running {
            return None;
        }
        entry.loaded_agent.clone()
    }

    pub fn infer_agent_response_with_trace(
        &self,
        agent_id: &str,
        messages: Vec<OpenAIMessage>,
    ) -> Result<Vec<OpenAIMessage>> {
        let agent = self.running_agent(agent_id).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "agent '{}' is not running",
                agent_id
            ))
        })?;
        agent.infer_response_with_trace(messages)
    }

    pub async fn infer_agent_response_streaming(
        &self,
        agent_id: &str,
        messages: Vec<OpenAIMessage>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> Result<Vec<OpenAIMessage>> {
        let agent = self.running_agent(agent_id).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "agent '{}' is not running",
                agent_id
            ))
        })?;
        agent
            .infer_response_streaming_with_trace(messages, token_tx)
            .await
    }

    pub async fn start_agent(
        &self,
        agent: AgentConfig,
        connections: Vec<ConnectionConfig>,
        on_finish: Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>,
        task_id: Option<String>,
    ) -> Result<()> {
        self.stop_agent(&agent.id).await?;
        let loaded_agent = Arc::new(LoadedInferenceAgent::load(&agent, &connections)?);

        self.update_state(
            &agent.id,
            AgentRuntimeState {
                status: AgentRuntimeStatus::Starting,
                started_at: None,
                last_error: None,
            },
        );

        match &agent.agent_type {
            AgentType::QqChat(config) => {
                let on_finish_shared: OnFinishShared = Arc::new(Mutex::new(on_finish));
                let task = qq_chat_agent::spawn(
                    self,
                    agent.clone(),
                    config.clone(),
                    connections,
                    Arc::clone(&on_finish_shared),
                    task_id.unwrap_or_default(),
                )
                .await?;
                let started_at = Local::now().to_rfc3339();
                let mut guard = self.inner.lock().unwrap();
                let entry = guard.entry(agent.id.clone()).or_default();
                entry.loaded_agent = Some(Arc::clone(&loaded_agent));
                entry.state = AgentRuntimeState {
                    status: AgentRuntimeStatus::Running,
                    started_at: Some(started_at),
                    last_error: None,
                };
                entry.task = Some(task);
                entry.on_finish = on_finish_shared;
                Ok(())
            }
            AgentType::HttpStream(config) => {
                let on_finish_shared: OnFinishShared = Arc::new(Mutex::new(on_finish));
                let task = http_stream_agent::spawn(
                    self,
                    agent.clone(),
                    config.clone(),
                    Arc::clone(&on_finish_shared),
                    task_id.unwrap_or_default(),
                )
                .await?;
                let started_at = Local::now().to_rfc3339();
                let mut guard = self.inner.lock().unwrap();
                let entry = guard.entry(agent.id.clone()).or_default();
                entry.loaded_agent = Some(Arc::clone(&loaded_agent));
                entry.state = AgentRuntimeState {
                    status: AgentRuntimeStatus::Running,
                    started_at: Some(started_at),
                    last_error: None,
                };
                entry.task = Some(task);
                entry.on_finish = on_finish_shared;
                Ok(())
            }
        }
    }

    pub async fn stop_agent(&self, agent_id: &str) -> Result<()> {
        let (task, on_finish_shared) = {
            let mut guard = self.inner.lock().unwrap();
            match guard.get_mut(agent_id) {
                Some(entry) => (entry.task.take(), Arc::clone(&entry.on_finish)),
                None => (None, Arc::new(Mutex::new(None))),
            }
        };
        // Call on_finish before aborting (winner-takes-all via Mutex).
        if let Some(cb) = on_finish_shared.lock().unwrap().take() {
            cb(false, None);
        }
        if let Some(task) = task {
            task.abort();
        }
        self.update_state(
            agent_id,
            AgentRuntimeState {
                status: AgentRuntimeStatus::Stopped,
                started_at: None,
                last_error: None,
            },
        );
        Ok(())
    }

    pub async fn auto_start_enabled_agents(&self) {
        let agents = match load_agents() {
            Ok(agents) => agents,
            Err(err) => {
                error!("Failed to load agents for auto start: {err}");
                return;
            }
        };
        let connections = match load_connections() {
            Ok(connections) => connections,
            Err(err) => {
                error!("Failed to load connections for auto start: {err}");
                return;
            }
        };

        for agent in agents
            .into_iter()
            .filter(|agent| agent.enabled && agent.auto_start)
        {
            if let Err(err) = self
                .start_agent(agent.clone(), connections.clone(), None, None)
                .await
            {
                error!("Failed to auto start agent '{}': {}", agent.name, err);
            }
        }
    }

    pub(crate) fn update_state(&self, agent_id: &str, state: AgentRuntimeState) {
        let mut guard = self.inner.lock().unwrap();
        let entry = guard.entry(agent_id.to_string()).or_default();
        entry.state = state;
        if entry.state.status != AgentRuntimeStatus::Running {
            entry.loaded_agent = None;
            entry.task = None;
        }
    }
}
