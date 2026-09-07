use std::sync::Arc;

use async_trait::async_trait;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::qq_brain::QqBrainOutput;
use super::run_blocking;
use crate::qq_chat::logging::QqChatTaskTrace;
use crate::tools::{AfterBrainAgent, QqReplyReviewRequest, QqReplyReviewResult};

/// Inputs of the QQ after-brain procedure (reply review and rewrite).
///
/// `request.candidate_message` is only read for standalone reviews that run without a turn
/// brain (e.g. style-learning feedback); inside a turn chain the candidate comes from the
/// brain procedure's output in [`ProcedureContext::procedure_outputs`] and the field is
/// ignored.
pub(crate) struct QqAfterBrainContext<'a> {
    pub(crate) review_llm: &'a Arc<dyn LLMBase>,
    pub(crate) rewrite_llm: &'a Arc<dyn LLMBase>,
    pub(crate) reply_system_prompt: Option<&'a str>,
    pub(crate) request: QqReplyReviewRequest,
    pub(crate) trace: &'a QqChatTaskTrace,
}

/// QQ after-brain procedure: reviews the candidate reply and rewrites it when unsafe.
///
/// Wraps the existing `AfterBrainAgent` implementation. The review result decides the final
/// outgoing message, so the procedure is `Blocking` and its errors propagate to the turn.
pub(crate) struct QqAfterBrain<'a> {
    context: QqAfterBrainContext<'a>,
}

impl<'a> QqAfterBrain<'a> {
    pub(crate) fn new(context: QqAfterBrainContext<'a>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Procedure for QqAfterBrain<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_after_brain",
            name: "QQ Reply Review",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let candidate_message = match context.find_output::<QqBrainOutput>() {
            Some(brain_output) => match brain_output.final_reply_text.clone() {
                // A missing or explicit-[no_reply] candidate is not reviewed.
                Some(text)
                    if !zihuan_core::agent::utils::string_utils::is_no_reply_directive(&text) =>
                {
                    text
                }
                _ => return Ok(ProcedureOutput::none()),
            },
            // Standalone review without a turn brain: use the caller-provided candidate.
            None => self.context.request.candidate_message.clone(),
        };
        let mut request = self.context.request.clone();
        request.candidate_message = candidate_message;
        let result = AfterBrainAgent::run(
            self.context.review_llm,
            self.context.rewrite_llm,
            self.context.reply_system_prompt,
            &request,
            self.context.trace,
        )?;
        Ok(ProcedureOutput::of(result))
    }
}

/// Execute the QQ after-brain procedure and return the review result for the turn.
pub(crate) fn run_after_brain(
    shared: &mut ProcedureContext,
    context: QqAfterBrainContext<'_>,
) -> Result<QqReplyReviewResult> {
    let output = run_blocking(Arc::new(QqAfterBrain::new(context)), shared)?;
    output.cloned::<QqReplyReviewResult>().ok_or_else(|| {
        Error::ValidationError("after-brain procedure produced no review result".to_string())
    })
}
