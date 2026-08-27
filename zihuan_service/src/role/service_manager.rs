use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::error;
use zihuan_core::inference::system_config::{load_role_services, RoleServiceConfig, RoleServiceType};
use serde::Serialize;
use zihuan_core::storage::{load_connections, ConnectionConfig};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zihuan_core::agent::tools::ToolCallingObserver;
use zihuan_core::error::Result;
use zihuan_core::llm::{LLMMessage, StreamToken};
use zihuan_core::task_context::AgentTaskRuntime;

use crate::role::{InferenceToolProvider, RoleBrainAgent};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleServiceRuntimeStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoleServiceRuntimeInfo {
    #[serde(rename = "agent_id")]
    pub role_service_id: String,
    pub instance_id: Option<String>,
    pub status: RoleServiceRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RoleServiceRuntimeState {
    pub instance_id: Option<String>,
    pub status: RoleServiceRuntimeStatus,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for RoleServiceRuntimeState {
    fn default() -> Self {
        Self {
            instance_id: None,
            status: RoleServiceRuntimeStatus::Stopped,
            started_at: None,
            last_error: None,
        }
    }
}

pub(super) type OnFinishShared = Arc<Mutex<Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>>>;

pub(super) struct RoleServiceRuntimeEntry {
    pub role_service: Option<Arc<RoleBrainAgent>>,
    pub state: RoleServiceRuntimeState,
    pub task: Option<JoinHandle<()>>,
    pub on_finish: OnFinishShared,
}

impl Default for RoleServiceRuntimeEntry {
    fn default() -> Self {
        Self {
            role_service: None,
            state: RoleServiceRuntimeState::default(),
            task: None,
            on_finish: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone, Default)]
pub struct RoleServiceManager {
    pub(super) inner: Arc<Mutex<HashMap<String, RoleServiceRuntimeEntry>>>,
}

impl RoleServiceManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn runtime_info(&self, role_service_id: &str) -> RoleServiceRuntimeInfo {
        let state = self
            .inner
            .lock()
            .unwrap()
            .get(role_service_id)
            .map(|entry| entry.state.clone())
            .unwrap_or_default();
        RoleServiceRuntimeInfo {
            role_service_id: role_service_id.to_string(),
            instance_id: state.instance_id,
            status: state.status,
            started_at: state.started_at,
            last_error: state.last_error,
        }
    }

    pub fn running_role_service(&self, role_service_id: &str) -> Option<Arc<RoleBrainAgent>> {
        let guard = self.inner.lock().unwrap();
        let entry = guard.get(role_service_id)?;
        if entry.state.status != RoleServiceRuntimeStatus::Running {
            return None;
        }
        entry.role_service.clone()
    }

    pub fn infer_role_response_with_trace(
        &self,
        role_service_id: &str,
        messages: Vec<LLMMessage>,
    ) -> Result<Vec<LLMMessage>> {
        let agent = self.running_role_service(role_service_id).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!("role service '{}' is not running", role_service_id))
        })?;
        agent.infer_response_with_trace(messages)
    }

    pub async fn infer_role_response_streaming(
        &self,
        role_service_id: &str,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
    ) -> Result<(Vec<LLMMessage>, zihuan_core::agent::tools::ToolCallingStopReason)> {
        self.infer_role_response_streaming_with_model(role_service_id, messages, token_tx, observer, None, None, None, None, None)
            .await
    }

    pub async fn infer_role_response_streaming_with_model(
        &self,
        role_service_id: &str,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        model_config_id: Option<&str>,
        thinking_type: Option<zihuan_core::inference::system_config::ThinkingType>,
        reasoning_effort: Option<zihuan_core::inference::system_config::ReasoningEffort>,
        workspace_path: Option<String>,
        session_id: Option<String>,
    ) -> Result<(Vec<LLMMessage>, zihuan_core::agent::tools::ToolCallingStopReason)> {
        let agent = self.running_role_service(role_service_id).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!("role service '{}' is not running", role_service_id))
        })?;
        if let Some(model_id) = model_config_id {
            let llm_refs = zihuan_core::inference::system_config::load_llm_refs()?;
            let mut llm_config = zihuan_core::agent::resource_resolver::resolve_llm_service_config(
                Some(model_id),
                &llm_refs,
                &agent.agent().name,
            )?;
            if let Some(override_value) = thinking_type {
                llm_config.thinking_type = Some(override_value);
            }
            if let Some(override_value) = reasoning_effort {
                llm_config.reasoning_effort = Some(override_value);
            }
            let llm = zihuan_core::agent::resource_resolver::build_llm_model(&llm_config)?;
            agent
                .infer_response_streaming_with_trace_and_llm(messages, token_tx, observer, llm, workspace_path, session_id)
                .await
        } else {
            agent
                .infer_response_streaming_with_trace(messages, token_tx, observer, workspace_path, session_id)
                .await
        }
    }

    pub async fn start_role_service(
        &self,
        agent: &RoleServiceConfig,
        connections: Vec<ConnectionConfig>,
        on_finish: Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>,
        task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    ) -> Result<()> {
        self.stop_role_service(&agent.id).await?;
        let start_result: Result<()> = async {
            let llm_refs = zihuan_core::inference::system_config::load_llm_refs()?;
            let tool_provider = build_role_tool_provider(&agent, &connections)?;
            let role_service = Arc::new(RoleBrainAgent::load_with_tools(&agent, &llm_refs, tool_provider)?);

            self.update_state(
                &agent.id,
                RoleServiceRuntimeState {
                    instance_id: None,
                    status: RoleServiceRuntimeStatus::Starting,
                    started_at: None,
                    last_error: None,
                },
            );

            let runtime_instance_id = Uuid::new_v4().to_string();

            match &agent.role_service_type {
                RoleServiceType::QqChat(config) => {
                    let on_finish_shared: OnFinishShared = Arc::new(Mutex::new(on_finish));
                    let manager = self.clone();
                    let agent_id_for_callback = agent.id.clone();
                    let callback = Arc::new(move |success: bool, error_message: Option<String>| {
                        manager.update_state(
                            &agent_id_for_callback,
                            RoleServiceRuntimeState {
                                instance_id: None,
                                status: if success { RoleServiceRuntimeStatus::Stopped } else { RoleServiceRuntimeStatus::Error },
                                started_at: None,
                                last_error: error_message,
                            },
                        );
                    });
                    let task = zihuan_ims_agent::qq_chat::spawn(
                        agent.clone(),
                        config.clone(),
                        connections,
                        callback,
                        task_runtime.clone(),
                    )
                    .await?;
                    let started_at = Local::now().to_rfc3339();
                    let mut guard = self.inner.lock().unwrap();
                    let entry = guard.entry(agent.id.clone()).or_default();
                    entry.role_service = Some(Arc::clone(&role_service));
                    entry.state = RoleServiceRuntimeState {
                        instance_id: Some(runtime_instance_id),
                        status: RoleServiceRuntimeStatus::Running,
                        started_at: Some(started_at),
                        last_error: None,
                    };
                    entry.task = Some(task);
                    entry.on_finish = on_finish_shared;
                    Ok(())
                }
                RoleServiceType::Workspace(_config) => {
                    let started_at = Local::now().to_rfc3339();
                    let mut guard = self.inner.lock().unwrap();
                    let entry = guard.entry(agent.id.clone()).or_default();
                    entry.role_service = Some(Arc::clone(&role_service));
                    entry.state = RoleServiceRuntimeState {
                        instance_id: Some(runtime_instance_id),
                        status: RoleServiceRuntimeStatus::Running,
                        started_at: Some(started_at),
                        last_error: None,
                    };
                    entry.task = None;
                    entry.on_finish = Arc::new(Mutex::new(on_finish));
                    Ok(())
                }
            }
        }
        .await;

        if let Err(err) = &start_result {
            self.update_state(
                &agent.id,
                RoleServiceRuntimeState {
                    instance_id: None,
                    status: RoleServiceRuntimeStatus::Error,
                    started_at: None,
                    last_error: Some(err.to_string()),
                },
            );
        }

        start_result
    }

    pub async fn stop_role_service(&self, role_service_id: &str) -> Result<()> {
        let (task, on_finish_shared) = {
            let mut guard = self.inner.lock().unwrap();
            match guard.get_mut(role_service_id) {
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
            role_service_id,
            RoleServiceRuntimeState {
                instance_id: None,
                status: RoleServiceRuntimeStatus::Stopped,
                started_at: None,
                last_error: None,
            },
        );
        Ok(())
    }

    pub async fn auto_start_enabled_role_services(&self) {
        let role_services = match load_role_services() {
            Ok(role_services) => role_services,
            Err(err) => {
                error!("Failed to load role services for auto start: {err}");
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

        for agent in role_services.into_iter().filter(|agent| agent.enabled && agent.auto_start) {
            if let Err(err) = self
                .start_role_service(&agent, connections.clone(), None, None)
                .await
            {
                error!("Failed to auto start role service '{}': {}", agent.name, err);
            }
        }
    }

    pub(crate) fn update_state(&self, role_service_id: &str, state: RoleServiceRuntimeState) {
        let mut guard = self.inner.lock().unwrap();
        let entry = guard.entry(role_service_id.to_string()).or_default();
        entry.state = state;
        if entry.state.status != RoleServiceRuntimeStatus::Running {
            entry.role_service = None;
            entry.task = None;
        }
    }
}

pub fn build_role_tool_provider(
    agent: &RoleServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    match &agent.role_service_type {
        RoleServiceType::QqChat(config) => zihuan_ims_agent::qq_chat::load_inference_tool_provider(agent, config, connections),
        RoleServiceType::Workspace(config) => {
            zihuan_workspace_agent::workspace_agent_service::load_inference_tool_provider(agent, config, connections)
        }
    }
}
