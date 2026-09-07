use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::agent::tools::ToolCallingObserver;
use zihuan_core::agent::AgentCancellation;
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::model_inference::model_config::{ReasoningEffort, ThinkingType};
use zihuan_core::role::brain_agent::{BrainAgent, ContextCompactionObserver};
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

/// Request-scoped LLM pair: the main brain model and the optional image-understanding override.
type ResolvedLLMs = (Arc<dyn LLMBase>, Option<Arc<dyn LLMBase>>);

/// The turn's brain invocation as a Procedure (documents/procedure.md).
///
/// **Design:** Created per turn by the transport adapter, which captures everything this brain
/// run needs (brain handle, message list, model overrides, cancellation, protocol observers).
/// [`WorkspaceBrain::run`] resolves the request-scoped LLMs and drives the streaming turn
/// through the [`BrainAgent`] trait; its [`ProcedureOutput`] carries
/// `(Vec<LLMMessage>, ToolCallingStopReason)` for the transport adapter and the following
/// procedures.
pub struct WorkspaceBrain {
    agent: Arc<dyn BrainAgent>,
    messages: Mutex<Option<Vec<LLMMessage>>>,
    session_id: String,
    workspace_path: Option<String>,
    model_config_id: Option<String>,
    image_understand_model_config_id: Option<String>,
    thinking_type: Option<ThinkingType>,
    reasoning_effort: Option<ReasoningEffort>,
    cancellation: Option<Arc<dyn AgentCancellation>>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
    compaction_observer: Option<ContextCompactionObserver>,
}

impl WorkspaceBrain {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent: Arc<dyn BrainAgent>,
        messages: Vec<LLMMessage>,
        session_id: String,
        workspace_path: Option<String>,
        model_config_id: Option<String>,
        image_understand_model_config_id: Option<String>,
        thinking_type: Option<ThinkingType>,
        reasoning_effort: Option<ReasoningEffort>,
        cancellation: Option<Arc<dyn AgentCancellation>>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
    ) -> Self {
        Self {
            agent,
            messages: Mutex::new(Some(messages)),
            session_id,
            workspace_path,
            model_config_id,
            image_understand_model_config_id,
            thinking_type,
            reasoning_effort,
            cancellation,
            observer,
            compaction_observer,
        }
    }

    /// Resolve the request-scoped LLMs: the per-request model override (with thinking /
    /// reasoning overrides) or the brain's own configured model, plus the optional
    /// image-understanding override.
    fn resolve_llms(&self) -> Result<ResolvedLLMs> {
        let llm_refs = load_llm_refs()?;
        let llm = match &self.model_config_id {
            Some(model_id) => {
                let mut llm_config =
                    resolve_llm_service_config(Some(model_id), &llm_refs, self.agent.name())?;
                if let Some(override_value) = self.thinking_type.clone() {
                    llm_config.thinking_type = Some(override_value);
                }
                if let Some(override_value) = self.reasoning_effort.clone() {
                    llm_config.reasoning_effort = Some(override_value);
                }
                build_llm_model(&llm_config)?
            }
            None => self.agent.llm(),
        };
        let image_understand_llm = match &self.image_understand_model_config_id {
            Some(model_id) => {
                let llm_config =
                    resolve_llm_service_config(Some(model_id), &llm_refs, self.agent.name())?;
                Some(build_llm_model(&llm_config)?)
            }
            None => None,
        };
        Ok((llm, image_understand_llm))
    }
}

#[async_trait]
impl Procedure for WorkspaceBrain {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "workspace_brain",
            name: "Workspace Brain",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let messages = self.messages.lock().unwrap().take().ok_or_else(|| {
            Error::ValidationError("workspace brain messages already consumed".to_string())
        })?;
        let sink = context.transport_out.clone().ok_or_else(|| {
            Error::ValidationError("workspace brain requires a transport out sink".to_string())
        })?;

        let (llm, image_understand_llm) = self.resolve_llms()?;
        let (output_messages, stop_reason) = self
            .agent
            .run_streaming(
                messages,
                sink.token_sender(),
                self.observer.clone(),
                self.compaction_observer.clone(),
                llm,
                image_understand_llm,
                self.workspace_path.clone(),
                Some(self.session_id.clone()),
                self.cancellation.clone(),
            )
            .await?;
        Ok(ProcedureOutput::of((output_messages, stop_reason)))
    }
}
