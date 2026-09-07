use std::sync::Arc;

use async_trait::async_trait;
use zihuan_core::agent::tools::ToolCallingStopReason;
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::{InferenceParam, LLMMessage};
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::qq_brain::QqBrainOutput;
use crate::qq_chat::logging::QqChatTaskTrace;

/// The meta-query turn's brain invocation as a Procedure (documents/procedure.md).
///
/// **Design:** Meta queries (tool list, system prompt, model name) bypass the tool-calling
/// loop: the brain is a single direct LLM inference over the pre-fetched context, producing the
/// same [`QqBrainOutput`] payload the main brain produces, so the shared AfterBrain review and
/// the transport adapter consume both paths uniformly.
pub(crate) struct QqMetaQueryBrain<'a> {
    llm: &'a Arc<dyn LLMBase>,
    trace: &'a QqChatTaskTrace,
    meta_messages: Vec<LLMMessage>,
}

impl<'a> QqMetaQueryBrain<'a> {
    pub(crate) fn new(
        llm: &'a Arc<dyn LLMBase>,
        trace: &'a QqChatTaskTrace,
        meta_messages: Vec<LLMMessage>,
    ) -> Self {
        Self { llm, trace, meta_messages }
    }
}

#[async_trait]
impl Procedure for QqMetaQueryBrain<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_meta_query_brain",
            name: "QQ Meta Query Brain",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        self.trace.mark_llm_request_started();
        let response = self.llm.inference(&InferenceParam {
            messages: &self.meta_messages,
            tools: None,
        });
        let candidate_message = response.content_text_owned().unwrap_or_default();
        let candidate_message = candidate_message.trim();
        if candidate_message.is_empty() {
            return Ok(ProcedureOutput::of(QqBrainOutput {
                final_reply_text: None,
                suppress_send: false,
                brain_output: vec![response],
                stop_reason: ToolCallingStopReason::Done,
            }));
        }

        self.trace.record_llm_result_parsed(Some(candidate_message));
        let suppress_send =
            zihuan_core::agent::utils::string_utils::is_no_reply_directive(candidate_message);
        Ok(ProcedureOutput::of(QqBrainOutput {
            final_reply_text: Some(candidate_message.to_string()),
            suppress_send,
            brain_output: vec![response],
            stop_reason: ToolCallingStopReason::Done,
        }))
    }
}
