//! Planer owns routing. Each instance has a FIFO root BUS and a nested request BUS.
use super::{state::AppState, ws::WsBroadcast};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Notify};
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::agent::tools::{
    AsyncToolOutput, Tool, ToolCallingObserver, ToolCallingRequest, ToolCallingResult,
    ToolCallingStopReason, ToolExecutionOutput, ToolExecutionResource,
};
use zihuan_core::agent::{Agent, AgentCancellation, AgentContext};
use zihuan_core::config::{
    llm_refs::load_llm_refs,
    planers::{load_planers, PlanerConfig},
    role_services::load_role_services,
};
use zihuan_core::error::Result;
use zihuan_core::message_part::MessagePart;
use zihuan_core::model_inference::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole, StreamToken};
use zihuan_core::role::planer::{PlanerCall, PlanerContext, PlanerFuture, PlanerHost};
use zihuan_core::role::service_config::RoleServiceKind;
use zihuan_core::task_context::{
    AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};
use zihuan_core::workspace::AskUserRequest;
use zihuan_service::role::planer_agent::{PlanerAgent, PlanerAgentConfig};

/// Planer 的调度说明：选择 Role、以普通事件触发，并说明 IMS 目标的投递方式。
const PLANER_SEND_GUIDANCE: &str = "\
Pick a Role for the task and use planer_send to deliver the full message. A Role handles it \
as an ordinary event with Planer as the source identity. No operation type is needed; for an \
IMS Role, target_type and target_id name the delivery target: when provided, the body the Role \
produces is reviewed, formatted and delivered to that target through the same pipeline as an \
ordinary reply, and the real delivery result is returned; when omitted, only the Role's body \
text is returned and nothing is delivered. For a Workspace session, the Role's reply text is \
returned. When the task depends on an image attached to this conversation, pass that image's \
media_id in media_ids: the image itself then travels with the task and the Role can send it. \
Describing an image in prompt text never transfers it. A planer_send call runs serially, waits \
for the target to finish and then returns the final result; do not send the same task twice. \
Use ask_user when more information is needed, then continue with the original Workspace \
session_id. Human approval can only be completed by the user in the target Workspace UI. \
Never expose other members' names, IDs, directories or internal routing to a Role.";

/// planer_send 的工具说明：IMS 目标提供时投递并返回真实发送结果。
const PLANER_SEND_DESCRIPTION: &str = "\
Send a message to one of your Roles as the Planer, triggering its normal processing, and wait \
for the result. No operation is needed; for an IMS Role, target_type/target_id name the delivery \
target (group chat / private chat); when provided, the body the Role produces is reviewed, \
formatted and delivered to that target through the same pipeline as an ordinary reply, and the \
real delivery result is returned; when omitted, only the body text is returned and nothing is \
delivered. media_ids forwards conversation images (their media_id markers) with the task so \
the Role can send them; describing them in prompt alone does not. For a Workspace, use \
session_id to continue the conversation and get the reply text.";

#[derive(Clone, Serialize, Deserialize)]
struct SessionOwner {
    context: PlanerContext,
    workspace_path: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct PlanerTask {
    task_id: String,
    root_id: String,
    planer_id: String,
    parent_task_id: Option<String>,
    target_role_id: String,
    status: String,
    session_id: Option<String>,
    result: Option<Value>,
    error: Option<String>,
    #[serde(skip)]
    cancellation: Option<Arc<AtomicBool>>,
}
#[derive(Default, Serialize, Deserialize)]
struct Records {
    tasks: BTreeMap<String, PlanerTask>,
    sessions: HashMap<String, SessionOwner>,
    #[serde(default)]
    cooperations: HashMap<String, Vec<LLMMessage>>,
}
struct BusRequest {
    context: PlanerContext,
    messages: Vec<LLMMessage>,
    tokens: Option<mpsc::UnboundedSender<StreamToken>>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
    reply: oneshot::Sender<Result<ToolCallingResult>>,
}
struct RunningPlaner {
    roots: mpsc::UnboundedSender<BusRequest>,
    active_bus: Mutex<Option<mpsc::UnboundedSender<BusRequest>>>,
    stopped: Arc<AtomicBool>,
    wake: Arc<Notify>,
}
struct Chain {
    planer_id: String,
    context: PlanerContext,
    nested: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<BusRequest>>>,
    active_task: Arc<Mutex<Option<String>>>,
    send_lock: Arc<tokio::sync::Mutex<()>>,
    stopped: Arc<AtomicBool>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
}
pub struct PlanerRuntime {
    state: Arc<AppState>,
    broadcast: WsBroadcast,
    tasks: Arc<dyn AgentTaskRuntime>,
    records: Mutex<Records>,
    running: Mutex<HashMap<String, Arc<RunningPlaner>>>,
    nested: Mutex<HashMap<String, mpsc::UnboundedSender<BusRequest>>>,
}
static RUNTIME: OnceLock<Arc<PlanerRuntime>> = OnceLock::new();
pub fn shared() -> Result<Arc<PlanerRuntime>> {
    RUNTIME
        .get()
        .cloned()
        .ok_or_else(|| zihuan_core::string_error!("Planer runtime unavailable"))
}
fn store_path() -> std::path::PathBuf {
    zihuan_core::system_config::application_data_dir().join("planer_tasks.json")
}
fn persist(records: &Records) -> Result<()> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec(records)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}
fn terminal(status: &str) -> bool {
    matches!(status, "success" | "failed" | "cancelled" | "interrupted" | "waiting_input")
}
struct Cancellation {
    stopped: Arc<AtomicBool>,
    task: Option<Arc<AtomicBool>>,
}
impl AgentCancellation for Cancellation {
    fn is_cancelled(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
            || self.task.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }
}
pub fn initialize(
    state: Arc<AppState>,
    broadcast: WsBroadcast,
    tasks: Arc<dyn AgentTaskRuntime>,
) -> Result<()> {
    let mut records: Records = if store_path().exists() {
        serde_json::from_slice(&std::fs::read(store_path())?)?
    } else {
        Records::default()
    };
    for task in records.tasks.values_mut() {
        if !matches!(task.status.as_str(), "success" | "failed" | "cancelled" | "interrupted") {
            task.status = "interrupted".into();
            task.error = Some("服务重启，任务中断；不会自动重放".into());
        }
    }
    persist(&records)?;
    let runtime = Arc::new(PlanerRuntime {
        state,
        broadcast,
        tasks,
        records: Mutex::new(records),
        running: Mutex::new(HashMap::new()),
        nested: Mutex::new(HashMap::new()),
    });
    RUNTIME
        .set(runtime.clone())
        .map_err(|_| zihuan_core::string_error!("Planer already initialized"))?;
    zihuan_core::role::planer::register_host(runtime.clone());
    for config in load_planers()?.into_iter().filter(|p| p.enabled && p.auto_start) {
        if let Err(e) = runtime.start(&config.config_id) {
            log::error!("Planer auto start {}: {e}", config.name);
        }
    }
    Ok(())
}
impl PlanerRuntime {
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }
    pub fn config(&self, id: &str) -> Result<PlanerConfig> {
        load_planers()?
            .into_iter()
            .find(|p| p.config_id == id)
            .ok_or_else(|| zihuan_core::string_error!("Planer 不存在"))
    }
    pub fn start(self: &Arc<Self>, id: &str) -> Result<()> {
        let config = self.config(id)?;
        if !config.enabled {
            return Err(zihuan_core::string_error!("Planer 已禁用"));
        }
        build_llm_model(&resolve_llm_service_config(Some(&config.llm_ref_id), &load_llm_refs()?)?)?;
        let mut running = self.running.lock().unwrap();
        if running.contains_key(id) {
            return Ok(());
        }
        let (tx, mut rx) = mpsc::unbounded_channel::<BusRequest>();
        let instance = Arc::new(RunningPlaner {
            roots: tx,
            active_bus: Mutex::new(None),
            stopped: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
        });
        running.insert(id.into(), instance.clone());
        drop(running);
        let runtime = self.clone();
        let planer_id = id.to_string();
        tokio::spawn(async move {
            loop {
                let request = tokio::select! { biased;
                    _ = instance.wake.notified() => { if instance.stopped.load(Ordering::Relaxed) { break; } else { continue; } },
                    request = rx.recv() => match request { Some(request) => request, None => break }
                };
                if instance.stopped.load(Ordering::Relaxed) {
                    let _ = request.reply.send(Err(zihuan_core::string_error!("Planer 已停止")));
                    break;
                }
                let (nested_tx, nested_rx) = mpsc::unbounded_channel();
                let key = format!("{}:{}", planer_id, request.context.root_id);
                runtime.nested.lock().unwrap().insert(key.clone(), nested_tx.clone());
                *instance.active_bus.lock().unwrap() = Some(nested_tx);
                let chain = Arc::new(Chain {
                    planer_id: planer_id.clone(),
                    context: request.context.clone(),
                    nested: Arc::new(tokio::sync::Mutex::new(nested_rx)),
                    active_task: Arc::new(Mutex::new(None)),
                    send_lock: Arc::new(tokio::sync::Mutex::new(())),
                    stopped: instance.stopped.clone(),
                    observer: request.observer.clone(),
                });
                let receiver = chain.nested.clone();
                let result = runtime.run_agent(chain, request.messages, request.tokens).await;
                *instance.active_bus.lock().unwrap() = None;
                // Requests received just as inference finished become ordinary FIFO work.
                {
                    let mut pending = receiver.lock().await;
                    pending.close();
                    while let Ok(request) = pending.try_recv() {
                        let _ = instance.roots.send(request);
                    }
                }
                runtime.nested.lock().unwrap().remove(&key);
                let _ = request.reply.send(result);
            }
            while let Ok(request) = rx.try_recv() {
                let _ = request.reply.send(Err(zihuan_core::string_error!("Planer 已停止")));
            }
        });
        Ok(())
    }
    pub fn stop(&self, id: &str) -> Result<()> {
        if let Some(instance) = self.running.lock().unwrap().remove(id) {
            instance.stopped.store(true, Ordering::Relaxed);
            instance.wake.notify_waiters();
            instance.wake.notify_one();
        }
        let mut records = self.records.lock().unwrap();
        for task in records.tasks.values_mut().filter(|t| t.planer_id == id && !terminal(&t.status))
        {
            if let Some(c) = &task.cancellation {
                c.store(true, Ordering::Relaxed);
            }
            task.status = "cancelled".into();
            task.error = Some("Planer 已停止；已发送的消息不会撤回".into());
            if let Some(session_id) = &task.session_id {
                self.state.tasks.lock().unwrap().stop_workspace_chat_task(session_id, None);
            }
        }
        persist(&records)?;
        Ok(())
    }
    pub async fn chat(
        self: &Arc<Self>,
        planer_id: &str,
        session_id: &str,
        messages: Vec<LLMMessage>,
        tokens: Option<mpsc::UnboundedSender<StreamToken>>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        cancellation: Arc<AtomicBool>,
    ) -> Result<ToolCallingResult> {
        let mut context = PlanerContext::root("", session_id);
        context.visited.clear();
        context.planer_id = planer_id.into();
        context.cancellation = Some(cancellation);
        self.submit(planer_id, context, messages, tokens, observer).await
    }
    async fn submit(
        &self,
        id: &str,
        context: PlanerContext,
        messages: Vec<LLMMessage>,
        tokens: Option<mpsc::UnboundedSender<StreamToken>>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
    ) -> Result<ToolCallingResult> {
        let config = self.config(id)?;
        if !config.enabled {
            return Err(zihuan_core::string_error!("Planer 已禁用"));
        }
        let instance = self
            .running
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| zihuan_core::string_error!("所属 Planer 未运行"))?;
        let (reply, result) = oneshot::channel();
        let nested = if context.role_id.is_empty() {
            None
        } else {
            self.nested
                .lock()
                .unwrap()
                .get(&format!("{id}:{}", context.root_id))
                .cloned()
                .or_else(|| instance.active_bus.lock().unwrap().clone())
        };
        let request = BusRequest {
            context,
            messages,
            tokens,
            observer,
            reply,
        };
        match nested {
            Some(sender) => {
                if let Err(error) = sender.send(request) {
                    instance
                        .roots
                        .send(error.0)
                        .map_err(|_| zihuan_core::string_error!("Planer BUS 已关闭"))?;
                }
            }
            None => instance
                .roots
                .send(request)
                .map_err(|_| zihuan_core::string_error!("Planer BUS 已关闭"))?,
        };
        result.await.map_err(|_| zihuan_core::string_error!("Planer 请求已中断"))?
    }
    fn role_catalog(&self, id: &str) -> Result<Value> {
        let config = self.config(id)?;
        let roles = load_role_services()?;
        Ok(Value::Array(config.members.iter().filter_map(|member| {
            let role = roles.iter().find(|r| r.id == member.role_id)?;
            let running = role.enabled && self.state.role_service_manager.running_role_service(&role.id).is_some();
            Some(json!({"role_id":role.id,"name":role.name,"type":role.role_service_type.kind,"description":member.description,"running":running}))
        }).collect()))
    }
    fn run_agent(
        self: &Arc<Self>,
        chain: Arc<Chain>,
        messages: Vec<LLMMessage>,
        tokens: Option<mpsc::UnboundedSender<StreamToken>>,
    ) -> PlanerFuture<Result<ToolCallingResult>> {
        let runtime = self.clone();
        let mut messages = messages;
        annotate_planer_media_hints(&mut messages);
        Box::pin(async move {
            let config = runtime.config(&chain.planer_id)?;
            let llm = build_llm_model(&resolve_llm_service_config(
                Some(&config.llm_ref_id),
                &load_llm_refs()?,
            )?)?;
            let tools = ["planer_send", "ask_user"]
                .into_iter()
                .map(|name| {
                    Arc::new(PlanerTool {
                        name,
                        runtime: runtime.clone(),
                        chain: chain.clone(),
                    }) as Arc<dyn Tool>
                })
                .collect();
            let system_prompt = format!(
                "You are the Planer, an independent dispatching agent: {}. {}\n\
                 You orchestrate your Roles only through BUS tools and never do file or command work yourself. Members: {}.\n\
                 {PLANER_SEND_GUIDANCE} Roles this collaboration chain must not re-enter: {}.",
                config.name,
                config.prompt,
                runtime.role_catalog(&chain.planer_id)?,
                json!(chain.context.visited)
            );
            let agent = PlanerAgent::new(PlanerAgentConfig { llm, system_prompt, tools })?;
            let (tokens, drain) = if let Some(tokens) = tokens {
                (tokens, None)
            } else {
                let (tx, mut rx) = mpsc::unbounded_channel();
                (tx, Some(tokio::spawn(async move { while rx.recv().await.is_some() {} })))
            };
            let context = AgentContext {
                token_tx: Some(tokens),
                observer: chain.observer.clone(),
                cancellation: Some(Arc::new(Cancellation {
                    stopped: chain.stopped.clone(),
                    task: chain.context.cancellation.clone(),
                })),
                ..Default::default()
            };
            let mut execution =
                Box::pin(agent.run_streaming(context, ToolCallingRequest { messages }));
            let (output, reason) = loop {
                tokio::select! {
                    result = &mut execution => { let result = result?; break (result.messages, result.stop_reason); },
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if chain.stopped.load(Ordering::Relaxed) || chain.context.is_cancelled() {
                            if let Some(drain) = drain { drain.abort(); }
                            let active = chain.active_task.lock().unwrap().clone();
                            if let Some(id) = active { let _ = runtime.cancel(&chain, &id); }
                            return Err(zihuan_core::string_error!("Planer 请求已取消"));
                        }
                    }
                }
            };
            if let Some(drain) = drain {
                drain.abort();
            }
            Ok(ToolCallingResult { messages: output, stop_reason: reason })
        })
    }
    fn query(&self, chain: &Chain, id: &str) -> Result<Value> {
        let records = self.records.lock().unwrap();
        let task = records
            .tasks
            .get(id)
            .filter(|t| t.planer_id == chain.planer_id && t.root_id == chain.context.root_id)
            .ok_or_else(|| zihuan_core::string_error!("任务不存在或不属于当前协作链"))?;
        let mut value = serde_json::to_value(task)?;
        if let Some(info) = self.tasks.query_task(id) {
            value["progress"] = json!(info.progress);
        }
        Ok(value)
    }
    fn set_status(&self, id: &str, status: &str) {
        let mut records = self.records.lock().unwrap();
        if let Some(task) = records.tasks.get_mut(id).filter(|t| !terminal(&t.status)) {
            task.status = status.into();
        }
        if let Err(e) = persist(&records) {
            log::error!("Planer persist: {e}");
        }
    }
    async fn send(self: &Arc<Self>, chain: Arc<Chain>, call: PlanerCall) -> Result<Value> {
        // Agent-layer tool calls may run in parallel; each handoff is serialized.
        let _serial = chain.send_lock.clone().lock_owned().await;
        if chain.active_task.lock().unwrap().is_some() {
            return Err(zihuan_core::string_error!("上一次发送尚未结束"));
        }
        if chain.stopped.load(Ordering::Relaxed) || chain.context.is_cancelled() {
            return Err(zihuan_core::string_error!("Planer 已停止或请求已取消"));
        }
        if call.prompt.trim().is_empty() {
            return Err(zihuan_core::string_error!("prompt 必填"));
        }
        let config = self.config(&chain.planer_id)?;
        if !config.enabled {
            return Err(zihuan_core::string_error!("Planer 已禁用"));
        }
        if !chain.context.role_id.is_empty()
            && !config.members.iter().any(|m| m.role_id == chain.context.role_id)
        {
            return Err(zihuan_core::string_error!("来源 Role 已不属于当前 Planer"));
        }
        let member = config
            .members
            .iter()
            .find(|m| m.role_id == call.target_role_id)
            .ok_or_else(|| zihuan_core::string_error!("目标 Role 不属于当前 Planer"))?;
        let mut next = chain.context.enter(&call.target_role_id)?;
        next.planer_id = chain.planer_id.clone();
        let roles = load_role_services()?;
        let role = roles
            .iter()
            .find(|r| r.id == call.target_role_id && r.enabled)
            .ok_or_else(|| zihuan_core::string_error!("目标 Role 不存在或已禁用"))?;
        if self.state.role_service_manager.running_role_service(&role.id).is_none() {
            return Err(zihuan_core::string_error!("目标 Role 未运行"));
        }
        let is_workspace = role.role_service_type.kind == RoleServiceKind::Workspace;
        if !is_workspace && role.role_service_type.kind != RoleServiceKind::QqChat {
            return Err(zihuan_core::string_error!("目标 Role 不支持消息处理"));
        }
        if !is_workspace && call.session_id.is_some() {
            return Err(zihuan_core::string_error!("session_id 仅用于 Workspace"));
        }
        match (call.target_type.as_deref(), call.target_id.as_deref()) {
            (None, None) => {}
            (Some("private" | "group"), Some(id)) if !is_workspace && !id.trim().is_empty() => {}
            _ => {
                return Err(zihuan_core::string_error!(
                    "IMS 参考会话需要同时提供有效的 target_type 和 target_id"
                ))
            }
        }
        // Conversation images travel by media_id: resolve them against the Planer session's
        // persisted records before dispatching, so a Role receives the actual image instead
        // of a text description of it. The Planer chat scopes its context as `root("",
        // session_id)`, so the root_id carries one leading colon; a Role-initiated chain has
        // no chat session behind its root_id and the lookup reports the ids as missing.
        let planer_media = if is_workspace || call.media_ids.is_empty() {
            HashMap::new()
        } else {
            let session_id = chain.context.root_id.trim_start_matches(':');
            let media = super::chat::collect_session_image_media(session_id, &call.media_ids);
            let missing: Vec<&str> = call
                .media_ids
                .iter()
                .map(|id| id.trim())
                .filter(|id| !id.is_empty() && !media.contains_key(*id))
                .collect();
            if !missing.is_empty() {
                return Err(zihuan_core::string_error!(
                    "media_ids 在当前会话中不存在: {}；请使用对话中出现的 media_id",
                    missing.join(", ")
                ));
            }
            media
        };
        let (session_id, path) = {
            let records = self.records.lock().unwrap();
            if is_workspace {
                if let Some(id) = &call.session_id {
                    let owner = records
                        .sessions
                        .get(id)
                        .filter(|o| {
                            o.context.root_id == next.root_id
                                && o.context.planer_id == next.planer_id
                                && o.context.role_id == next.role_id
                        })
                        .ok_or_else(|| {
                            zihuan_core::string_error!("只能续聊当前协作链创建的 Workspace 会话")
                        })?;
                    (Some(id.clone()), owner.workspace_path.clone())
                } else {
                    let id = uuid::Uuid::new_v4().to_string();
                    let path = member.workspace_path.clone().unwrap_or_default();
                    if !std::path::Path::new(&path).is_dir() {
                        return Err(zihuan_core::string_error!("Workspace 默认工作目录不存在"));
                    }
                    (Some(id), path)
                }
            } else {
                (None, String::new())
            }
        };
        let handle = self.tasks.start_task(AgentTaskRequest {
            task_name: format!("Planer: {} → {}", config.name, role.name),
            agent_id: role.id.clone(),
            agent_name: role.name.clone(),
            user_ip: None,
            owner_id: Some(next.root_id.clone()),
            task_db_connection_id: None,
        });
        let id = handle.task_id.clone();
        let cancellation = Arc::new(AtomicBool::new(false));
        next.task_id = Some(id.clone());
        next.cancellation = Some(cancellation.clone());
        {
            let mut records = self.records.lock().unwrap();
            if let Some(session_id) = &session_id {
                records.sessions.insert(
                    session_id.clone(),
                    SessionOwner {
                        context: next.clone(),
                        workspace_path: path.clone(),
                    },
                );
            }
            records.tasks.insert(
                id.clone(),
                PlanerTask {
                    task_id: id.clone(),
                    root_id: next.root_id.clone(),
                    planer_id: config.config_id,
                    parent_task_id: chain.context.task_id.clone(),
                    target_role_id: role.id.clone(),
                    status: "queued".into(),
                    session_id: session_id.clone(),
                    result: None,
                    error: None,
                    cancellation: Some(cancellation.clone()),
                },
            );
            if let Err(e) = persist(&records) {
                records.tasks.remove(&id);
                handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Failed),
                    result_summary: None,
                    error_message: Some(e.to_string()),
                });
                return Err(e);
            }
        }
        *chain.active_task.lock().unwrap() = Some(id.clone());
        let runtime = self.clone();
        let task_id = id.clone();
        let task = tokio::spawn(async move {
            runtime.set_status(&task_id, "running");
            let execution_runtime = runtime.clone();
            let execution = async move {
                if is_workspace {
                    super::chat::execute_planer_chat(
                        execution_runtime.state.clone(),
                        execution_runtime.broadcast.clone(),
                        &call.target_role_id,
                        session_id.as_deref().unwrap(),
                        &path,
                        &call.prompt,
                        next,
                    )
                    .await
                } else {
                    let target = call.target_role_id.clone();
                    zihuan_ims_service::qq_chat::planer::execute(&target, call, planer_media, next)
                        .await
                }
            };
            let result = tokio::spawn(execution).await.unwrap_or_else(|e| {
                Err(zihuan_core::string_error!("Planer task interrupted: {e}"))
            });
            let mut records = runtime.records.lock().unwrap();
            if let Some(record) = records.tasks.get_mut(&task_id) {
                if cancellation.load(Ordering::Relaxed) || record.status == "cancelled" {
                    record.status = "cancelled".into();
                } else {
                    match &result {
                        Ok(v) if v.get("success") == Some(&Value::Bool(false)) => {
                            record.status = "failed".into();
                            record.error = Some(v["error"].as_str().unwrap_or("发送未完成").into());
                        }
                        Ok(v) if !v["pending_ask_user"].is_null() => {
                            record.status = "waiting_input".into()
                        }
                        Ok(_) => record.status = "success".into(),
                        Err(e) => {
                            record.status = "failed".into();
                            record.error = Some(e.to_string());
                        }
                    }
                }
                record.result = result.as_ref().ok().cloned();
                handle.finish(AgentTaskResult {
                    status: Some(
                        if record.status == "success" || record.status == "waiting_input" {
                            AgentTaskStatus::Success
                        } else {
                            AgentTaskStatus::Failed
                        },
                    ),
                    result_summary: record.result.as_ref().map(Value::to_string),
                    error_message: record.error.clone(),
                });
            }
            if let Err(e) = persist(&records) {
                log::error!("Planer persist: {e}");
            }
            drop(records);
            result
        });
        let result = self.await_task(&chain, &id, task).await;
        *chain.active_task.lock().unwrap() = None;
        result
    }
    async fn await_task(
        self: &Arc<Self>,
        chain: &Arc<Chain>,
        id: &str,
        mut task: tokio::task::JoinHandle<Result<Value>>,
    ) -> Result<Value> {
        loop {
            tokio::select! {
                joined = &mut task => {
                    if let Err(error) = joined {
                        let mut records = self.records.lock().unwrap();
                        if let Some(record) = records.tasks.get_mut(id).filter(|r| !terminal(&r.status)) {
                            record.status = "failed".into();
                            record.error = Some(format!("Planer task interrupted: {error}"));
                        }
                        if let Err(e) = persist(&records) {
                            log::error!("Planer persist: {e}");
                        }
                    }
                    return self.query(chain, id);
                }
                request = async { chain.nested.lock().await.recv().await } => {
                    let Some(request) = request else {
                        return Err(zihuan_core::string_error!("Planer BUS 已关闭"));
                    };
                    let mut context = request.context;
                    for ancestor in &chain.context.visited {
                        if !context.visited.contains(ancestor) {
                            context.visited.push(ancestor.clone());
                        }
                    }
                    let child = Arc::new(Chain {
                        planer_id: chain.planer_id.clone(),
                        context,
                        nested: chain.nested.clone(),
                        active_task: Arc::new(Mutex::new(None)),
                        send_lock: Arc::new(tokio::sync::Mutex::new(())),
                        stopped: chain.stopped.clone(),
                        observer: chain.observer.clone(),
                    });
                    let result = self.run_agent(child, request.messages, request.tokens).await;
                    let _ = request.reply.send(result);
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if chain.stopped.load(Ordering::Relaxed) || chain.context.is_cancelled() {
                        let _ = self.cancel(chain, id);
                        return Err(zihuan_core::string_error!("Planer 请求已取消"));
                    }
                }
            }
        }
    }
    /// Business text is model-authored, so do not trust it to omit routing metadata.
    fn role_result(&self, mut value: Value, root_id: &str) -> Result<Value> {
        let mut secrets: Vec<(String, &'static str)> = Vec::new();
        let mut names = Vec::new();
        for role in load_role_services()? {
            secrets.push((role.id, "[执行方]"));
            if !role.name.trim().is_empty() {
                let escaped = regex::escape(&role.name);
                let pattern = if role.name.is_ascii() {
                    format!(r"\b{escaped}\b")
                } else {
                    escaped
                };
                names.push(
                    regex::Regex::new(&pattern).map_err(|e| zihuan_core::string_error!("{e}"))?,
                );
            }
        }
        for config in load_planers()? {
            secrets.push((config.config_id, "[调度方]"));
            for member in config.members {
                if let Some(path) = member.workspace_path.filter(|p| p.len() > 1) {
                    secrets.push((path, "[工作目录]"));
                }
            }
        }
        {
            let records = self.records.lock().unwrap();
            secrets.push((root_id.to_string(), "[协作上下文]"));
            for task in records.tasks.values().filter(|t| t.root_id == root_id) {
                secrets.push((task.task_id.clone(), "[任务]"));
                if let Some(id) = &task.session_id {
                    secrets.push((id.clone(), "[会话]"));
                }
            }
        }
        secrets.sort_by_key(|(text, _)| std::cmp::Reverse(text.len()));
        fn redact(value: &mut Value, secrets: &[(String, &str)], names: &[regex::Regex]) {
            match value {
                Value::String(text) => {
                    for name in names {
                        *text = name.replace_all(text, "[执行方]").into_owned();
                    }
                    for (secret, replacement) in secrets {
                        if !secret.is_empty() {
                            *text = text.replace(secret, replacement);
                        }
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        redact(item, secrets, names);
                    }
                }
                Value::Object(items) => {
                    for item in items.values_mut() {
                        redact(item, secrets, names);
                    }
                }
                _ => {}
            }
        }
        redact(&mut value, &secrets, &names);
        Ok(value)
    }
    fn cancel(&self, chain: &Chain, id: &str) -> Result<Value> {
        self.query(chain, id)?;
        let mut records = self.records.lock().unwrap();
        let mut ids = vec![id.to_string()];
        loop {
            let children: Vec<_> = records
                .tasks
                .values()
                .filter(|t| {
                    t.parent_task_id.as_ref().is_some_and(|p| ids.contains(p))
                        && !ids.contains(&t.task_id)
                })
                .map(|t| t.task_id.clone())
                .collect();
            if children.is_empty() {
                break;
            }
            ids.extend(children);
        }
        for task in records
            .tasks
            .values_mut()
            .filter(|t| ids.contains(&t.task_id) && !terminal(&t.status))
        {
            if let Some(c) = &task.cancellation {
                c.store(true, Ordering::Relaxed);
            }
            task.status = "cancelled".into();
            task.error = Some("任务已取消；已发生的外部操作不会回滚".into());
            if let Some(session) = &task.session_id {
                self.state.tasks.lock().unwrap().stop_workspace_chat_task(session, None);
            }
        }
        persist(&records)?;
        drop(records);
        self.query(chain, id)
    }
}
impl PlanerHost for PlanerRuntime {
    fn owns(&self, role_id: &str) -> bool {
        load_planers()
            .is_ok_and(|ps| ps.iter().any(|p| p.members.iter().any(|m| m.role_id == role_id)))
    }
    fn session_context(&self, role_id: &str, session_id: &str) -> Option<PlanerContext> {
        self.records
            .lock()
            .unwrap()
            .sessions
            .get(session_id)
            .filter(|o| o.context.role_id == role_id)
            .map(|o| o.context.clone())
    }
    fn request(&self, mut context: PlanerContext, prompt: String) -> PlanerFuture<Result<Value>> {
        Box::pin(async move {
            if prompt.trim().is_empty() {
                return Err(zihuan_core::string_error!("prompt 必填"));
            }
            if context.is_cancelled() {
                return Err(zihuan_core::string_error!("任务已取消"));
            }
            let runtime = shared()?;
            let owner = load_planers()?
                .into_iter()
                .find(|p| p.members.iter().any(|m| m.role_id == context.role_id))
                .ok_or_else(|| zihuan_core::string_error!("Role 未归属 Planer"))?;
            if !context.planer_id.is_empty() && context.planer_id != owner.config_id {
                return Err(zihuan_core::string_error!("Role 归属已变更"));
            }
            context.planer_id = owner.config_id.clone();
            if context.cancellation.is_none() {
                context.cancellation = Some(Arc::new(AtomicBool::new(false)));
            }
            let cancellation = context.cancellation.clone().unwrap();
            let source = context.role_id.clone();
            let root_id = context.root_id.clone();
            let local = context.local_cancellation.clone();
            let history_key =
                format!("{}:{}:{}", owner.config_id, context.root_id, context.role_id);
            let mut history = runtime
                .records
                .lock()
                .unwrap()
                .cooperations
                .get(&history_key)
                .cloned()
                .unwrap_or_default();
            history.push(LLMMessage::user(prompt));
            let mut execution =
                Box::pin(runtime.submit(&owner.config_id, context, history.clone(), None, None));
            let result = loop {
                tokio::select! {
                    result = &mut execution => break result?,
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if local.as_ref().is_some_and(|c| c.load(Ordering::Relaxed)) || runtime.state.role_service_manager.running_role_service(&source).is_none() {
                            cancellation.store(true, Ordering::Relaxed);
                            return Err(zihuan_core::string_error!("来源任务已取消或 Role 已停止"));
                        }
                    }
                }
            };
            if let ToolCallingStopReason::TransportError(error) = &result.stop_reason {
                return Err(zihuan_core::string_error!("{error}"));
            }
            history.extend(result.messages.clone());
            {
                let mut records = runtime.records.lock().unwrap();
                records.cooperations.insert(history_key, history);
                persist(&records)?;
            }
            let text = result
                .messages
                .iter()
                .rev()
                .find(|m| m.role == MessageRole::Assistant)
                .and_then(LLMMessage::content_text_owned)
                .unwrap_or_default();
            let question = match result.stop_reason {
                ToolCallingStopReason::AwaitUserInput(q)
                | ToolCallingStopReason::ToolCallLimitReached(q) => Some(q),
                _ => None,
            };
            runtime.role_result(json!({"text":text,"pending_ask_user":question}), &root_id)
        })
    }
}
struct PlanerTool {
    name: &'static str,
    runtime: Arc<PlanerRuntime>,
    chain: Arc<Chain>,
}

/// Appends one media_id hint part to the latest user message when the conversation carries
/// image parts. A multimodal request delivers the image itself as an image_url part, which
/// never shows the id — without this hint the model cannot name the image in planer_send's
/// media_ids, and the attachment would silently stop at the Planer.
fn annotate_planer_media_hints(messages: &mut [LLMMessage]) {
    let mut media_ids = Vec::new();
    for message in messages.iter() {
        if !matches!(message.role, MessageRole::User) {
            continue;
        }
        for part in &message.parts {
            if let MessagePart::Image { media } = part {
                let id = media.media_id.trim();
                if !id.is_empty() && !media_ids.iter().any(|existing| existing == id) {
                    media_ids.push(id.to_string());
                }
            }
        }
    }
    if media_ids.is_empty() {
        return;
    }
    let Some(latest_user) = messages.iter_mut().rev().find(|m| matches!(m.role, MessageRole::User))
    else {
        return;
    };
    latest_user.parts.push(MessagePart::text(format!(
        "[Image attachments]\n{}\nTo let a Role send these images, pass their media_ids in \
         planer_send's media_ids argument; describing an image in prompt text does not \
         transfer it.",
        media_ids
            .iter()
            .map(|id| format!("- media_id={id}"))
            .collect::<Vec<_>>()
            .join("\n")
    )));
}
impl Tool for PlanerTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        let parameters = match self.name {
            "planer_send" => {
                json!({"type":"object","properties":{"target_role_id":{"type":"string"},"prompt":{"type":"string"},"session_id":{"type":"string"},"target_type":{"type":"string","enum":["private","group"]},"target_id":{"type":"string"},"media_ids":{"type":"array","items":{"type":"string"},"description":"media_id of conversation images to forward with the task"}},"required":["target_role_id","prompt"],"additionalProperties":false})
            }
            "ask_user" => {
                json!({"type":"object","properties":{"question":{"type":"string"},"options":{"type":"array","items":{"type":"string"}}},"required":["question"]})
            }
            _ => json!({"type":"object","properties":{}}),
        };
        Arc::new(StaticFunctionToolSpec { name: self.name, description: match self.name {
            "planer_send" => PLANER_SEND_DESCRIPTION,
            "ask_user" => "向用户提问并暂停，或汇总子任务需要补充的信息。审批必须在目标 Workspace 界面由用户处理。",
            _ => "未知的 Planer 工具。",
        }, parameters })
    }
    fn execution_resource(&self, _: &Value) -> ToolExecutionResource {
        if self.name == "ask_user" {
            ToolExecutionResource::Exclusive
        } else {
            ToolExecutionResource::Concurrent
        }
    }
    fn returns_failure_details_to_agent(&self) -> bool {
        true
    }
    fn execute_async(&self, _: &str, args: &Value) -> Option<AsyncToolOutput> {
        let runtime = self.runtime.clone();
        let chain = self.chain.clone();
        let args = args.clone();
        let name = self.name;
        Some(Box::pin(async move {
            if name == "ask_user" {
                let question = args["question"].as_str().unwrap_or_default().trim().to_string();
                if question.is_empty() {
                    return ToolExecutionOutput::text(
                        json!({"ok":false,"error":"question 必填"}).to_string(),
                    );
                }
                return ToolExecutionOutput::ask_user(
                    json!({"ok":true,"question":question}).to_string(),
                    AskUserRequest {
                        question,
                        details: None,
                        placeholder: None,
                        options: args["options"].as_array().map(|a| {
                            a.iter().filter_map(|v| v.as_str().map(String::from)).collect()
                        }),
                        command_confirmation: None,
                        tool_call_limit: None,
                    },
                );
            }
            let result = match name {
                "planer_send" => match serde_json::from_value(args.clone()) {
                    Ok(call) => runtime.send(chain, call).await,
                    Err(e) => Err(e.into()),
                },
                _ => Err(zihuan_core::string_error!("Unknown Planer tool")),
            };
            ToolExecutionOutput::text(
                match result {
                    Ok(v) => v,
                    Err(e) => json!({"ok":false,"error":e.to_string()}),
                }
                .to_string(),
            )
        }))
    }
    fn execute(&self, content: &str, args: &Value) -> Result<String> {
        Ok(zihuan_core::runtime::block_async(self.execute_async(content, args).unwrap()).result)
    }
}
