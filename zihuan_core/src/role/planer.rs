//! The only cooperation boundary visible to a Role. Routing is host-owned.
use crate::agent::tools::{Tool, ToolExecutionOutput, ToolExecutionResource};
use crate::error::Result;
use crate::model_inference::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock, RwLock,
};

pub type PlanerFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanerContext {
    pub root_id: String,
    pub role_id: String,
    pub visited: Vec<String>,
    #[serde(default)]
    pub planer_id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(skip)]
    pub local_cancellation: Option<Arc<AtomicBool>>,
    #[serde(skip)]
    pub cancellation: Option<Arc<AtomicBool>>,
}
impl PlanerContext {
    pub fn root(role_id: &str, scope: &str) -> Self {
        Self {
            root_id: format!("{role_id}:{scope}"),
            role_id: role_id.into(),
            visited: vec![role_id.into()],
            planer_id: String::new(),
            task_id: None,
            local_cancellation: None,
            cancellation: None,
        }
    }
    pub fn enter(&self, role_id: &str) -> Result<Self> {
        if self.visited.iter().any(|id| id == role_id) {
            return Err(crate::string_error!("协作链不能重复进入已访问的 Role"));
        }
        let mut next = self.clone();
        next.role_id = role_id.into();
        next.visited.push(role_id.into());
        Ok(next)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
            || self.local_cancellation.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanerCall {
    pub target_role_id: String,
    pub prompt: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub target_type: Option<String>,
    #[serde(default)]
    pub target_id: Option<String>,
}
pub trait PlanerHost: Send + Sync {
    fn owns(&self, role_id: &str) -> bool;
    fn request(&self, context: PlanerContext, prompt: String) -> PlanerFuture<Result<Value>>;
    fn session_context(&self, role_id: &str, session_id: &str) -> Option<PlanerContext>;
}
fn host_slot() -> &'static RwLock<Option<Arc<dyn PlanerHost>>> {
    static HOST: OnceLock<RwLock<Option<Arc<dyn PlanerHost>>>> = OnceLock::new();
    HOST.get_or_init(Default::default)
}
pub fn register_host(host: Arc<dyn PlanerHost>) {
    *host_slot().write().unwrap() = Some(host);
}
fn host() -> Option<Arc<dyn PlanerHost>> {
    host_slot().read().unwrap().clone()
}
pub fn session_context(role_id: &str, session_id: &str) -> PlanerContext {
    host()
        .and_then(|h| h.session_context(role_id, session_id))
        .unwrap_or_else(|| PlanerContext::root(role_id, session_id))
}
pub fn prompt(role_id: &str) -> String {
    if host().is_some_and(|h| h.owns(role_id)) {
        "\n需要外部协助时，用 planer_request 向所属 Planer 描述完整需求。Planer 负责选择执行者，结果返回后继续当前任务。不要猜测或请求列出其他 Role、内部路由和配置。".into()
    } else {
        String::new()
    }
}
pub fn prompt_for_context(context: &PlanerContext) -> String {
    prompt(&context.role_id)
}
pub fn tools(context: PlanerContext) -> Vec<Box<dyn Tool>> {
    if !host().is_some_and(|h| h.owns(&context.role_id)) {
        return vec![];
    }
    vec![Box::new(PlanerRequestTool { context })]
}
struct PlanerRequestTool {
    context: PlanerContext,
}
impl Tool for PlanerRequestTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec { name: "planer_request", description: "向所属 Planer 请求协助。描述业务需求及约束，等待业务结果后继续当前回合；不要填写执行者或内部路由。",
            parameters: json!({"type":"object","properties":{"prompt":{"type":"string"}},"required":["prompt"],"additionalProperties":false}) })
    }
    fn execution_resource(&self, _: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Exclusive
    }
    fn returns_failure_details_to_agent(&self) -> bool {
        true
    }
    fn execute_async(&self, _: &str, args: &Value) -> Option<crate::agent::tools::AsyncToolOutput> {
        let context = self.context.clone();
        let prompt = args["prompt"].as_str().unwrap_or_default().to_string();
        Some(Box::pin(async move {
            let result = match host() {
                Some(h) => h.request(context, prompt).await,
                None => Err(crate::string_error!("Planer 未运行")),
            };
            match result {
                Ok(v) => ToolExecutionOutput::text(v.to_string()),
                Err(e) => {
                    ToolExecutionOutput::text(json!({"ok":false,"error":e.to_string()}).to_string())
                }
            }
        }))
    }
    fn execute(&self, content: &str, args: &Value) -> Result<String> {
        let future = self.execute_async(content, args).unwrap();
        Ok(
            crate::runtime::run_outside_async_runtime(|| crate::runtime::block_async(future))
                .result,
        )
    }
}
