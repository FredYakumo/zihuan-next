use std::sync::Arc;

use async_trait::async_trait;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
    ProcedurePhase,
};

use super::run_blocking;
use crate::qq_chat::logging::QqChatTaskTrace;
use crate::tools::{AfterBrainAgent, QqReplyReviewRequest, QqReplyReviewResult};

/// Inputs of the QQ after-brain procedure (reply review and rewrite).
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
            phase: ProcedurePhase::AfterBrain,
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        let context = &self.context;
        let result = AfterBrainAgent::run(
            context.review_llm,
            context.rewrite_llm,
            context.reply_system_prompt,
            &context.request,
            context.trace,
        )?;
        Ok(ProcedureOutput::of(result))
    }
}

/// Execute the QQ after-brain procedure and return the review result for the turn.
pub(crate) fn run_after_brain(
    shared: &ProcedureContext,
    context: QqAfterBrainContext<'_>,
) -> Result<QqReplyReviewResult> {
    let output = run_blocking(Arc::new(QqAfterBrain::new(context)), shared)?;
    output.cloned::<QqReplyReviewResult>().ok_or_else(|| {
        Error::ValidationError("after-brain procedure produced no review result".to_string())
    })
}
