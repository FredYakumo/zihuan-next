//! Rust scheduler kernel: owns persistence, due-task claiming, dispatch, and crash
//! recovery. Job bodies live in dynamic scripts (`scheduled_jobs/`) that receive the task
//! payload and call back into the host through the capability handlers in [`capabilities`].

mod capabilities;
mod job_script;

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use serde::Serialize;

use crate::data_refs::RelationalDbConnection;
use crate::error::Result;
use crate::graph::tool_spec::ToolDefinition;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::LLMMessage;
use crate::runtime::block_async;
use crate::scheduled_task::{self, ScheduledTaskEntry, ScheduledTaskStatus};
use crate::task_context::{
    scope_task_id, AgentTaskResult, AgentTaskStatus, ScheduledJobTaskRequest,
};

pub use job_script::{
    builtin_schedulers, delete_job_script, ensure_default_jobs, init_script_jobs,
    is_builtin_scheduler, is_builtin_script, read_job_script, reload_script_jobs, save_job_script,
    BuiltinScheduler, JobManifest, SavedJobScript, DREAM_TASK_NAME,
};

/// How often the kernel scans for due tasks.
const TICK_SECONDS: u64 = 5;

/// Per-service resources a job body may use while it runs. Registered by the owning
/// RoleService when it starts; scripts reach them only through capability handlers.
pub struct JobResources {
    pub agent_id: String,
    pub connection: RelationalDbConnection,
    pub llm: Arc<dyn LLMBase>,
    pub tool_definitions: Vec<ToolDefinition>,
    /// Loads the raw conversation history for one sender (process-local message cache).
    pub history_loader: Arc<dyn Fn(&str) -> Vec<LLMMessage> + Send + Sync>,
    /// Clears the conversation history for one sender.
    pub history_clearer: Arc<dyn Fn(&str) -> Result<()> + Send + Sync>,
}

/// Everything a job body sees for one firing.
pub struct JobRunContext {
    pub task: ScheduledTaskEntry,
    pub resources: Arc<JobResources>,
}

/// A scheduled job body. `run` executes on a blocking thread and returns the task summary;
/// the kernel owns the task lifecycle (claim before, finish after), so implementations
/// must not write the `scheduled_task` row themselves.
pub trait SchedulerJob: Send + Sync {
    fn run(&self, context: &JobRunContext) -> Result<Option<String>>;
}

#[derive(Default)]
struct RegistryState {
    /// `source_service` (agent id) -> the service's job resources.
    services: HashMap<String, Arc<JobResources>>,
    /// `task_name` -> the job body registered for it.
    jobs: HashMap<String, Arc<dyn SchedulerJob>>,
    manifests: HashMap<String, JobManifest>,
}

static REGISTRY: OnceLock<RwLock<RegistryState>> = OnceLock::new();
static KERNEL_STARTED: OnceLock<()> = OnceLock::new();

fn registry() -> &'static RwLock<RegistryState> {
    REGISTRY.get_or_init(|| RwLock::new(RegistryState::default()))
}

/// Keeps one service's job resources registered; dropping it unregisters the service.
/// Store it on the service instance so stop/abort of the service task also cleans up.
pub struct ServiceRegistrationGuard {
    source_service: String,
}

impl Drop for ServiceRegistrationGuard {
    fn drop(&mut self) {
        if registry().write().unwrap().services.remove(&self.source_service).is_some() {
            log::info!("[Scheduler] service unregistered: {}", self.source_service);
        }
    }
}

pub fn register_service(resources: JobResources) -> Result<ServiceRegistrationGuard> {
    let agent_id = resources.agent_id.clone();
    let connection = resources.connection.clone();
    // Running rows here are leftovers from a crashed previous process; the service is the
    // only executor for its own tasks, so requeueing at registration is safe.
    if let Err(error) = block_async(scheduled_task::requeue_running_tasks(&connection, &agent_id)) {
        log::warn!("[Scheduler] failed to requeue running tasks for service '{agent_id}': {error}");
    }
    registry()
        .write()
        .unwrap()
        .services
        .insert(agent_id.clone(), Arc::new(resources));
    log::info!("[Scheduler] service registered: {agent_id}");
    Ok(ServiceRegistrationGuard { source_service: agent_id })
}

pub fn register_job(manifest: JobManifest, job: Arc<dyn SchedulerJob>) {
    let task_name = manifest.task_name.clone();
    let mut state = registry().write().unwrap();
    if let Some(existing) = state.manifests.get(&task_name) {
        log::error!(
            "[Scheduler] job task_name 重复注册，覆盖先注册的 job: task_name '{}'（{} 被 {} 覆盖）",
            task_name,
            existing.script,
            manifest.script
        );
    }
    state.manifests.insert(task_name.clone(), manifest);
    state.jobs.insert(task_name.clone(), job);
    log::info!("[Scheduler] job registered: {task_name}");
}

/// Drops every registered job body, so a directory re-scan can rebuild the set from scratch
/// without tripping the duplicate-`task_name` error in [`register_job`]. Service registrations
/// are untouched: they describe which resources exist, not which jobs do.
pub(crate) fn clear_jobs() {
    let mut state = registry().write().unwrap();
    let removed = state.jobs.len();
    state.jobs.clear();
    state.manifests.clear();
    if removed > 0 {
        log::info!("[Scheduler] cleared {removed} registered job(s) for reload");
    }
}

/// Starts the dispatch loop; idempotent. Pending tasks that came due while the process was
/// down are picked up on the first ticks after their service re-registers.
pub fn start_scheduler_kernel() {
    if KERNEL_STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async move {
        log::info!("[Scheduler] kernel started");
        let mut ticker = tokio::time::interval(Duration::from_secs(TICK_SECONDS));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            tick().await;
        }
    });
}

async fn tick() {
    let services: Vec<Arc<JobResources>> =
        registry().read().unwrap().services.values().cloned().collect();
    for resources in services {
        let due = match scheduled_task::list_due_tasks(&resources.connection, chrono::Local::now())
            .await
        {
            Ok(due) => due,
            Err(error) => {
                log::warn!(
                    "[Scheduler] failed to list due tasks for service '{}': {error}",
                    resources.agent_id
                );
                continue;
            }
        };
        for task in due {
            let job = registry().read().unwrap().jobs.get(&task.task_name).cloned();
            let Some(job) = job else {
                log::warn!(
                    "[Scheduler] no job registered for task_name '{}'; task '{}' stays pending",
                    task.task_name,
                    task.id
                );
                continue;
            };
            match scheduled_task::claim_task(&resources.connection, &task.id).await {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    log::warn!("[Scheduler] failed to claim task '{}': {error}", task.id);
                    continue;
                }
            }
            // Mirror the run into task history so it shows up in the task manager. The
            // `scheduled_task` row stays authoritative for scheduling and crash recovery.
            let task_handle = match crate::command::global_task_runtime() {
                Some(runtime) => Some(runtime.start_scheduled_job_task(ScheduledJobTaskRequest {
                    task_name: task.task_name.clone(),
                    source_service: task.source_service.clone(),
                    triggered_by: task.triggered_by.clone(),
                })),
                None => {
                    log::warn!(
                        "[Scheduler] task runtime unavailable; scheduled task '{}' will not be recorded as a task",
                        task.id
                    );
                    None
                }
            };
            let context = JobRunContext {
                task: task.clone(),
                resources: Arc::clone(&resources),
            };
            let connection = resources.connection.clone();
            let task_id = task.id.clone();
            let job_task_id = task_handle.as_ref().map(|handle| handle.task_id.clone());
            tokio::spawn(async move {
                let outcome = tokio::task::spawn_blocking(move || {
                    std::panic::catch_unwind(AssertUnwindSafe(|| match &job_task_id {
                        Some(task_id) => scope_task_id(task_id.clone(), || job.run(&context)),
                        None => job.run(&context),
                    }))
                })
                .await;
                let (status, summary) = match outcome {
                    Ok(Ok(Ok(summary))) => (
                        ScheduledTaskStatus::Succeeded,
                        summary.or_else(|| Some("完成".to_string())),
                    ),
                    Ok(Ok(Err(error))) => (ScheduledTaskStatus::Failed, Some(error.to_string())),
                    Ok(Err(panic)) => (
                        ScheduledTaskStatus::Failed,
                        Some(format!("job panicked: {}", panic_message(&panic))),
                    ),
                    Err(error) => {
                        (ScheduledTaskStatus::Failed, Some(format!("job worker failed: {error}")))
                    }
                };
                if let Err(error) = scheduled_task::finish_task(
                    &connection,
                    &task_id,
                    status.clone(),
                    summary.as_deref(),
                )
                .await
                {
                    log::warn!("[Scheduler] failed to finish task {task_id}: {error}");
                }
                if let Some(handle) = task_handle {
                    handle.finish(match status {
                        ScheduledTaskStatus::Failed => AgentTaskResult {
                            status: Some(AgentTaskStatus::Failed),
                            result_summary: None,
                            error_message: summary,
                        },
                        _ => AgentTaskResult {
                            status: Some(AgentTaskStatus::Success),
                            result_summary: summary,
                            error_message: None,
                        },
                    });
                }
            });
        }
    }
}

fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// One registered job, as reported for diagnostics.
#[derive(Debug, Serialize)]
pub struct SchedulerJobStatus {
    pub task_name: String,
    pub script: String,
    pub entry: String,
    pub description: String,
    pub language: String,
    /// True when the job ships with the application rather than being operator-provided.
    pub builtin: bool,
}

/// A service that has registered its job resources with the scheduler. A service runs only the
/// tasks stored in its own database.
#[derive(Debug, Serialize)]
pub struct SchedulerServiceStatus {
    pub source_service: String,
    pub agent_id: String,
}

/// snapshot of the scheduler registry.
#[derive(Debug, Serialize)]
pub struct SchedulerStatus {
    pub jobs: Vec<SchedulerJobStatus>,
    pub services: Vec<SchedulerServiceStatus>,
}

/// snapshot of the scheduler registry for diagnostics.
pub fn status() -> SchedulerStatus {
    let state = registry().read().unwrap();
    let jobs = state
        .manifests
        .values()
        .map(|manifest| SchedulerJobStatus {
            task_name: manifest.task_name.clone(),
            script: manifest.script.clone(),
            entry: manifest.entry.clone(),
            description: manifest.description.clone(),
            language: script_language_name(Path::new(&manifest.script)),
            builtin: is_builtin_scheduler(&manifest.task_name),
        })
        .collect();
    let services = state
        .services
        .iter()
        .map(|(source_service, resources)| SchedulerServiceStatus {
            source_service: source_service.clone(),
            agent_id: resources.agent_id.clone(),
        })
        .collect();
    SchedulerStatus { jobs, services }
}

fn script_language_name(script: &Path) -> String {
    match dynamic_script_engine::ScriptLanguage::from_path(script) {
        Some(dynamic_script_engine::ScriptLanguage::JavaScript) => "javascript".to_string(),
        Some(dynamic_script_engine::ScriptLanguage::Python) => "python".to_string(),
        None => "unknown".to_string(),
    }
}
