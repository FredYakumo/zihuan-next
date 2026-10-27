use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use zihuan_core::agent::utils::string_utils::is_no_reply_directive;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;
use zihuan_core::graph::data_value::DataValue;
use zihuan_core::ims_bot_adapter::models::message::Message;
use zihuan_core::qq_context::emit_qq_context_stage;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::utils::reply_plan::{
    bubble_stage_summaries, build_reply_result, reply_batch_delays_ms, take_reply_directive,
    REPLY_TYPING_DELAY_MS_PER_CHAR,
};
use crate::qq_chat::collect_available_media_from_brain_output;
use crate::qq_chat::logging::{QqChatTaskTrace, QqChatTurnIdentity};
use crate::qq_chat::model::QqChatServiceReplyBatchBuilder;

use super::natural_language_reply::QqReplyReviewResult;
use super::qq_brain::QqBrainOutput;

/// Inputs of the QQ format-output procedure: the turn facts needed to turn the reviewed
/// reply text into protocol-compliant message batches.
///
/// The procedure replaces the inline "build reply" step that turn callers used to perform
/// after the review: @sender expansion, mention/image/forward protocol segments, line-based
/// multi-message dispatch, and the reply-quote directive all happen here.
pub(crate) struct QqFormatOutputContext<'a> {
    pub(crate) trace: &'a QqChatTaskTrace,
    /// Live context-view identity. When set, the formatted bubbles are listed as their own
    /// stage in the turn flow (Data Explorer / 数据检索上下文).
    pub(crate) identity: Option<QqChatTurnIdentity>,
    pub(crate) is_group: bool,
    pub(crate) sender_id: String,
    pub(crate) sender_nickname: String,
    pub(crate) sender_card: String,
    pub(crate) bot_id: String,
    pub(crate) bot_name: String,
    pub(crate) max_message_length: usize,
    pub(crate) trigger_message_id: Option<i64>,
    pub(crate) rdb_pool: Option<RelationalDbConnection>,
    pub(crate) reply_batch_builder: Option<&'a QqChatServiceReplyBatchBuilder>,
    /// Shared runtime values carrying the reply-quote directive written by brain tools.
    pub(crate) shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
}

/// Output of the QQ format-output procedure: the final sendable batches plus the visible
/// text the turn should record in history.
#[derive(Clone)]
pub(crate) struct QqFormatOutputOutput {
    pub(crate) batches: Vec<Vec<Message>>,
    /// Typing delay (ms) before each batch; entry 0 is always 0.
    pub(crate) batch_delays_ms: Vec<u64>,
    pub(crate) suppress_send: bool,
    pub(crate) final_message: Option<String>,
}

/// QQ format-output procedure: formats the reviewed reply into outbound message batches.
///
/// Runs after the reply review and decides the turn's final wire format, so the procedure
/// is `Blocking` and its errors propagate to the turn.
pub(crate) struct QqFormatOutput<'a> {
    context: QqFormatOutputContext<'a>,
}

impl<'a> QqFormatOutput<'a> {
    pub(crate) fn new(context: QqFormatOutputContext<'a>) -> Self {
        Self { context }
    }

    fn emit_stage(&self, output: &QqFormatOutputOutput, candidate: Option<&str>) {
        let detail = json!({
            "suppress_send": output.suppress_send,
            "final_message": candidate,
            "bubble_count": output.batches.len(),
            "typing_delay_ms_per_char": REPLY_TYPING_DELAY_MS_PER_CHAR,
            "bubbles": bubble_stage_summaries(&output.batches, &output.batch_delays_ms),
        });
        self.context.trace.record_graph_phase("格式化输出", detail.clone());
        if let Some(identity) = self.context.identity.as_ref() {
            emit_qq_context_stage(
                &identity.agent_id,
                &identity.sender_id,
                &identity.turn_id,
                "格式化输出",
                detail,
            );
        }
    }
}

#[async_trait]
impl Procedure for QqFormatOutput<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_format_output",
            name: "QQ Format Output",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let brain_output = context.find_output::<QqBrainOutput>().map(|value| (*value).clone());
        let review_result =
            context.find_output::<QqReplyReviewResult>().map(|value| (*value).clone());

        // The reviewed message wins; without a review (suppressed brain output) there is
        // nothing to format.
        let candidate =
            review_result.as_ref().map(|result| result.final_message.clone()).or_else(|| {
                brain_output
                    .as_ref()
                    .and_then(|output| output.final_reply_text.clone())
                    .filter(|text| !is_no_reply_directive(text))
            });
        let brain_suppressed =
            brain_output.as_ref().map(|output| output.suppress_send).unwrap_or(false);

        // Consume the directive even on suppressed turns so a stored reply-quote can never
        // leak into a later turn.
        let reply_directive = take_reply_directive(&self.context.shared_runtime_values);
        if brain_suppressed || candidate.is_none() {
            let output = QqFormatOutputOutput {
                batches: Vec::new(),
                batch_delays_ms: Vec::new(),
                suppress_send: true,
                final_message: None,
            };
            self.emit_stage(&output, None);
            return Ok(ProcedureOutput::of(output));
        }

        let candidate = candidate.unwrap();
        let available_media = brain_output
            .as_ref()
            .map(|output| collect_available_media_from_brain_output(&output.brain_output))
            .unwrap_or_default();
        let reply_result = build_reply_result(
            &candidate,
            self.context.is_group,
            &self.context.sender_id,
            &self.context.sender_nickname,
            &self.context.sender_card,
            &self.context.bot_id,
            &self.context.bot_name,
            self.context.max_message_length,
            reply_directive,
            self.context.trigger_message_id,
            available_media,
            self.context.rdb_pool.clone(),
            self.context.reply_batch_builder,
        )?;

        let output = QqFormatOutputOutput {
            batch_delays_ms: reply_batch_delays_ms(&reply_result.batches),
            batches: reply_result.batches,
            suppress_send: reply_result.suppress_send,
            final_message: Some(candidate),
        };
        self.emit_stage(&output, output.final_message.as_deref());
        Ok(ProcedureOutput::of(output))
    }
}
