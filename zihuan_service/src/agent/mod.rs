pub mod http_stream_agent;
pub mod inference;
pub mod qq_chat_agent;
pub mod tool_definitions;

mod agent_text_similarity;
mod classify_intent;
mod qq_chat_agent_core;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::error;
use model_inference::system_config::{load_agents, AgentConfig, AgentType};
use serde::Serialize;
use storage_handler::{load_connections, ConnectionConfig};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zihuan_core::error::Result;
use zihuan_core::llm::OpenAIMessage;
use zihuan_core::task_context::AgentTaskRuntime;

use self::inference::{InferenceToolProvider, LoadedInferenceAgent, StaticInferenceToolProvider};
use self::tool_definitions::build_enabled_tool_definitions;

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
            instance_id: state.instance_id,
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
        task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    ) -> Result<()> {
        self.stop_agent(&agent.id).await?;
        let llm_refs = model_inference::system_config::load_llm_refs()?;
        let tool_provider = build_inference_tool_provider(&agent, &connections)?;
        let loaded_agent = Arc::new(LoadedInferenceAgent::load_with_tools(
            &agent,
            &llm_refs,
            tool_provider,
        )?);

        self.update_state(
            &agent.id,
            AgentRuntimeState {
                instance_id: None,
                status: AgentRuntimeStatus::Starting,
                started_at: None,
                last_error: None,
            },
        );

        let runtime_instance_id = Uuid::new_v4().to_string();

        match &agent.agent_type {
            AgentType::QqChat(config) => {
                let on_finish_shared: OnFinishShared = Arc::new(Mutex::new(on_finish));
                let task = qq_chat_agent::spawn(
                    self,
                    agent.clone(),
                    config.clone(),
                    connections,
                    Arc::clone(&on_finish_shared),
                    task_runtime.clone(),
                )
                .await?;
                let started_at = Local::now().to_rfc3339();
                let mut guard = self.inner.lock().unwrap();
                let entry = guard.entry(agent.id.clone()).or_default();
                entry.loaded_agent = Some(Arc::clone(&loaded_agent));
                entry.state = AgentRuntimeState {
                    instance_id: Some(runtime_instance_id),
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
                    task_runtime.clone(),
                )
                .await?;
                let started_at = Local::now().to_rfc3339();
                let mut guard = self.inner.lock().unwrap();
                let entry = guard.entry(agent.id.clone()).or_default();
                entry.loaded_agent = Some(Arc::clone(&loaded_agent));
                entry.state = AgentRuntimeState {
                    instance_id: Some(runtime_instance_id),
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
                instance_id: None,
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

pub fn build_inference_tool_provider(
    agent: &AgentConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    match &agent.agent_type {
        AgentType::QqChat(config) => {
            qq_chat_agent::load_inference_tool_provider(agent, config, connections)
        }
        AgentType::HttpStream(_) => Ok(Arc::new(StaticInferenceToolProvider::new(
            build_enabled_tool_definitions(&agent.tools)?,
        ))),
    }
}
