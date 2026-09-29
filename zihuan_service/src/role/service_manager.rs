use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::error;
use serde::Serialize;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::agent::tool_definitions::build_enabled_tool_definitions;
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::config::role_services::load_role_services;
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    execute_procedure_chain, Procedure, ProcedureContext, ProcedureOutput,
};
use zihuan_core::role::service_config::RoleServiceConfig;
use zihuan_core::scheduler::{JobResources, ScheduledJobConfig, ServiceRegistrationGuard};
use zihuan_core::storage::{
    build_relational_db_connection_for_connection, load_connections, ConnectionConfig,
};
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_workspace_service::role_config::WorkspaceRoleServiceConfig;

use crate::role::{InferenceToolProvider, RoleAgent};

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

pub(super) type OnFinishShared =
    Arc<Mutex<Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>>>;

pub(super) struct RoleServiceRuntimeEntry {
    pub role_service: Option<Arc<RoleAgent>>,
    pub state: RoleServiceRuntimeState,
    pub task: Option<JoinHandle<()>>,
    pub on_finish: OnFinishShared,
    /// Keeps the workspace agent's scheduler job resources registered while it runs;
    /// dropping it unregisters the resources. QQ chat services hold their guard inside
    /// the spawned service task instead.
    pub scheduler: Option<ServiceRegistrationGuard>,
}

impl Default for RoleServiceRuntimeEntry {
    fn default() -> Self {
        Self {
            role_service: None,
            state: RoleServiceRuntimeState::default(),
            task: None,
            on_finish: Arc::new(Mutex::new(None)),
            scheduler: None,
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

    pub fn running_role_service(&self, role_service_id: &str) -> Option<Arc<RoleAgent>> {
        let guard = self.inner.lock().unwrap();
        let entry = guard.get(role_service_id)?;
        if entry.state.status != RoleServiceRuntimeStatus::Running {
            return None;
        }
        entry.role_service.clone()
    }

    /// Run one chat work unit's procedure chain (documents/procedure.md).
    ///
    /// **Design:** The chain is a plain `Vec` — execution order is the `Vec` order. The manager
    /// prepends the procedures owned by the agent's RoleService type (collected from the owning
    /// crate), then runs everything the transport adapter passed in — typically its per-turn
    /// Brain procedure — through [`execute_procedure_chain`]. Errors propagate so the adapter
    /// decides how to surface them. QQ Chat turns assemble and execute their chain inside the
    /// IMS pipeline instead, so nothing is collected for that type here.
    pub async fn run_procedure_chain(
        &self,
        agent: &RoleServiceConfig,
        mut context: ProcedureContext,
        mut procedures: Vec<Arc<dyn Procedure>>,
    ) -> Result<Vec<ProcedureOutput>> {
        let mut owned = if super::service_type_ext::is_workspace_agent(agent) {
            zihuan_workspace_service::procedure::collect(agent, &context)
        } else {
            Vec::new()
        };
        owned.append(&mut procedures);
        execute_procedure_chain(owned, &mut context).await
    }

    /// Execute one chat work unit as the RoleService procedure chain (documents/procedure.md):
    /// BeforeBrain procedures -> brain -> AfterBrain procedures.
    ///
    pub async fn start_role_service(
        &self,
        agent: &RoleServiceConfig,
        connections: Vec<ConnectionConfig>,
        on_finish: Option<Box<dyn FnOnce(bool, Option<String>) + Send + 'static>>,
        task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    ) -> Result<()> {
        self.stop_role_service(&agent.id).await?;
        let start_result: Result<()> = async {
            let llm_refs = load_llm_refs()?;
            let tool_provider = build_role_tool_provider(&agent, &connections)?;
            let role_service =
                Arc::new(RoleAgent::load_with_tools(&agent, &llm_refs, tool_provider)?);

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

            let is_qq_chat = super::service_type_ext::is_qq_chat_agent(agent);
            if is_qq_chat {
                let config = super::service_type_ext::qq_chat_of(&agent.role_service_type)?;
                let on_finish_shared: OnFinishShared = Arc::new(Mutex::new(on_finish));
                let manager = self.clone();
                let agent_id_for_callback = agent.id.clone();
                let callback = Arc::new(move |success: bool, error_message: Option<String>| {
                    manager.update_state(
                        &agent_id_for_callback,
                        RoleServiceRuntimeState {
                            instance_id: None,
                            status: if success {
                                RoleServiceRuntimeStatus::Stopped
                            } else {
                                RoleServiceRuntimeStatus::Error
                            },
                            started_at: None,
                            last_error: error_message,
                        },
                    );
                });
                let task = zihuan_ims_service::qq_chat::spawn(
                    agent.clone(),
                    config,
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
            } else {
                let config = super::service_type_ext::workspace_of(&agent.role_service_type)?;
                let scheduler_guard =
                    register_workspace_scheduler(agent, &config, &connections).await;
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
                entry.scheduler = scheduler_guard;
                Ok(())
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
            if let Err(err) = self.start_role_service(&agent, connections.clone(), None, None).await
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
            entry.scheduler = None;
        }
    }
}

/// Registers the workspace agent's scheduler job resources so its scheduled jobs can fire,
/// returning the guard that keeps the registration alive for as long as the entry does.
/// `None` when the agent configures no scheduled jobs; a resource that cannot be built only
/// logs, because scheduled jobs are auxiliary and must not block the agent from starting.
async fn register_workspace_scheduler(
    agent: &RoleServiceConfig,
    config: &WorkspaceRoleServiceConfig,
    connections: &[ConnectionConfig],
) -> Option<ServiceRegistrationGuard> {
    let scheduled_jobs = config.resolved_scheduled_jobs();
    if scheduled_jobs.is_empty() {
        return None;
    }
    match build_workspace_job_resources(agent, config, connections, scheduled_jobs).await {
        Ok(resources) => match zihuan_core::scheduler::register_service(resources) {
            Ok(registration) => Some(registration),
            Err(err) => {
                log::warn!(
                    "[Scheduler] failed to register scheduled jobs for agent '{}': {err}",
                    agent.name
                );
                None
            }
        },
        Err(err) => {
            log::warn!("[Scheduler] agent '{}' scheduled jobs unavailable: {err}", agent.name);
            None
        }
    }
}

async fn build_workspace_job_resources(
    agent: &RoleServiceConfig,
    config: &WorkspaceRoleServiceConfig,
    connections: &[ConnectionConfig],
    scheduled_jobs: Vec<ScheduledJobConfig>,
) -> Result<JobResources> {
    let rdb_id = config.resolved_rdb_id().ok_or_else(|| {
        zihuan_core::string_error!("scheduled jobs require a relational database connection")
    })?;
    let connection = build_relational_db_connection_for_connection(rdb_id, connections).await?;
    let llm_config =
        resolve_llm_service_config(config.llm_ref_id.as_deref(), &load_llm_refs()?, &agent.name)?;
    let llm = build_llm_model(&llm_config)?;
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;
    let memory = zihuan_workspace_service::workspace_agent_service::load_job_memory_resources(
        config,
        connections,
    );
    Ok(JobResources {
        agent_id: agent.id.clone(),
        connection,
        llm,
        tool_definitions,
        // Workspace session history is the persisted conversation; loading it gives job
        // scripts a real transcript, while clearing it is refused — a scheduled job must
        // not delete what the user sees.
        history_loader: Arc::new(|session_id: &str| {
            zihuan_core::chat_history::load_session_history_messages(session_id).unwrap_or_default()
        }),
        history_clearer: Arc::new(|_session_id: &str| {
            log::info!("[Scheduler] history.clear is a no-op for workspace agents");
            Ok(())
        }),
        memory,
        scheduled_jobs,
    })
}

pub fn build_role_tool_provider(
    agent: &RoleServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    if super::service_type_ext::is_qq_chat_agent(agent) {
        let config = super::service_type_ext::qq_chat_of(&agent.role_service_type)?;
        zihuan_ims_service::qq_chat::load_inference_tool_provider(agent, &config, connections)
    } else {
        let config = super::service_type_ext::workspace_of(&agent.role_service_type)?;
        zihuan_workspace_service::workspace_agent_service::load_inference_tool_provider(
            agent,
            &config,
            connections,
        )
    }
}
