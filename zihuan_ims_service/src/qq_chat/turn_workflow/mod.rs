//! Single-turn workflow (glossary: `turn`).
//!
//! [`run_turn`] prepares and dispatches one claimed turn; the dispatched paths (this
//! module's `normal_turn` and `meta_query_turn`) assemble their procedure chain
//! (documents/procedure.md) and execute it through `execute_blocking_procedure_chain`.
//! The data contracts ([`TurnInput`], [`TurnContext`]) and the chain segments both paths
//! share (after-brain, format-output constructors) are defined below; turn-lifecycle
//! steps shared by both paths (reply delivery, session-state write-back, scheduler
//! re-arm) live in [`finalization`].

mod finalization;
mod meta_query_turn;
mod normal_turn;
mod run_turn;

pub(crate) use run_turn::run_turn;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zihuan_core::ims_bot_adapter::models::MessageEvent;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::LLMMessage;

use super::core::{QqChatAgentService, QqChatAgentServiceContext};
use super::logging::{QqChatTaskTrace, QqChatTurnIdentity};
use super::user_input::PreparedCurrentTurnUserInput;
use crate::classify_intent::IntentCategory;
use crate::procedure::{
    QqAfterBrain, QqAfterBrainContext, QqFormatOutput, QqFormatOutputContext, QqReplyReviewRequest,
};
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::{QqChatEmotionDimensionConfig, QqChatRoleServiceConfig};
use crate::storage::message_rate_limit_store::MessageRateLimitCheckResult;
use zihuan_core::graph::DataValue;

/// Everything one claimed turn needs, bundled so [`run_turn`] takes a single parameter.
pub(crate) struct TurnInput<'a> {
    pub(crate) trace: &'a QqChatTaskTrace,
    pub(crate) event: &'a MessageEvent,
    pub(crate) sender_id: &'a str,
    pub(crate) target_id: &'a str,
    pub(crate) is_group: bool,
    pub(crate) bot_id: &'a str,
    pub(crate) message_rate_limit: Option<&'a MessageRateLimitCheckResult>,
    /// True when this turn replays queued steer messages after a previous turn in the same
    /// session; the context view then keeps both inside one block.
    pub(crate) continues_previous: bool,
    pub(crate) ctx: &'a QqChatAgentServiceContext<'a>,
}

/// The working state of one turn, built by [`run_turn`] after setup and consumed by the
/// dispatched path (the meta-query or the normal brain path).
pub(crate) struct TurnContext<'a> {
    pub(crate) service: &'a QqChatAgentService,
    pub(crate) trace: &'a QqChatTaskTrace,
    pub(crate) ctx: &'a QqChatAgentServiceContext<'a>,
    pub(crate) event: &'a MessageEvent,
    pub(crate) sender_id: &'a str,
    pub(crate) target_id: &'a str,
    pub(crate) is_group: bool,
    pub(crate) bot_id: &'a str,
    pub(crate) continues_previous: bool,
    /// The rate-limit warning prompt for the before-brain assembly, if a limit applied.
    pub(crate) message_rate_limit_warning: Option<&'a str>,
    pub(crate) turn_id: &'a str,
    pub(crate) prepared_input: PreparedCurrentTurnUserInput,
    pub(crate) inference_event: MessageEvent,
    /// The user text sent to inference; command passthrough may have replaced it.
    pub(crate) current_message: String,
    pub(crate) history_key: String,
    pub(crate) history: Vec<LLMMessage>,
    /// The turn's routed LLM (main vs math/programming) picked by intent classification.
    pub(crate) turn_llm: &'a Arc<dyn LLMBase>,
    pub(crate) role_config: QqChatRoleServiceConfig,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    pub(crate) turn_session_state: Arc<Mutex<QqChatSessionState>>,
}

/// The after-brain review/rewrite procedure, shared by the normal and meta-query chains.
/// The candidate message is filled from the brain output inside the procedure itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_after_brain<'a>(
    ctx: &'a QqChatAgentServiceContext<'_>,
    trace: &'a QqChatTaskTrace,
    is_group: bool,
    sender_id: &str,
    inference_event: &MessageEvent,
    session_state: QqChatSessionState,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
) -> QqAfterBrain<'a> {
    QqAfterBrain::new(QqAfterBrainContext {
        review_llm: ctx.intent_classification_llm,
        rewrite_llm: ctx.natural_language_reply_llm,
        reply_system_prompt: ctx.natural_language_reply_system_prompt,
        request: QqReplyReviewRequest {
            candidate_message: String::new(),
            is_group,
            bot_name: ctx.bot_name.to_string(),
            sender_id: sender_id.to_string(),
            sender_nickname: inference_event.sender.nickname.clone(),
            sender_card: inference_event.sender.card.clone(),
            session_state,
            emotion_dimensions,
            model_names: ctx.model_names.clone(),
        },
        trace,
    })
}

/// The format-output procedure, shared by the normal and meta-query chains: it turns the
/// reviewed reply text into delayed message batches for the transport.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_format_output<'a>(
    ctx: &'a QqChatAgentServiceContext<'_>,
    trace: &'a QqChatTaskTrace,
    turn_id: &str,
    is_group: bool,
    sender_id: &str,
    bot_id: &str,
    inference_event: &MessageEvent,
    shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
) -> QqFormatOutput<'a> {
    QqFormatOutput::new(QqFormatOutputContext {
        trace,
        identity: Some(QqChatTurnIdentity {
            agent_id: ctx.agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.to_string(),
        }),
        is_group,
        sender_id: sender_id.to_string(),
        sender_nickname: inference_event.sender.nickname.clone(),
        sender_card: inference_event.sender.card.clone(),
        bot_id: bot_id.to_string(),
        bot_name: ctx.bot_name.to_string(),
        max_message_length: ctx.max_message_length,
        trigger_message_id: Some(inference_event.message_id.clone()),
        rdb_pool: ctx.rdb_pool.cloned(),
        reply_batch_builder: ctx.reply_batch_builder,
        shared_runtime_values,
    })
}

/// Picks the turn's routed LLM: math/programming for complex problems and code, main otherwise.
fn selected_turn_llm<'a>(
    ctx: &'a QqChatAgentServiceContext<'_>,
    intent_category: IntentCategory,
) -> (&'a Arc<dyn LLMBase>, &'a str) {
    match intent_category {
        IntentCategory::SolveComplexProblem | IntentCategory::WriteCode => {
            (ctx.math_programming_llm, "math_programming")
        }
        _ => (ctx.llm, "main"),
    }
}

/// The rate-limit snapshot the context view shows for one turn: which rule applied
/// (user/group/default), its window and quota, and how much of it this message used.
fn rate_limit_stage_detail(result: Option<&MessageRateLimitCheckResult>) -> serde_json::Value {
    let Some(result) = result else {
        // No relational database: the runtime skips rate limiting entirely for this turn.
        return serde_json::json!({ "configured": false, "unlimited": true });
    };
    let rule = result.resolved_limit.as_ref().map(|resolved| &resolved.rule);
    serde_json::json!({
        "configured": rule.is_some(),
        "unlimited": rule.is_none_or(|rule| rule.is_effectively_unlimited()),
        "scope_type": result.resolved_limit.as_ref().map(|resolved| resolved.scope_type.as_str()),
        "window_unit": rule.and_then(|rule| rule.window_unit).map(|unit| unit.as_str()),
        "window_size": rule.map(|rule| rule.window_size),
        "max_calls": result.max_calls,
        "used_calls": result.used_calls_after_increment,
        "warning_level": result.warning_level,
    })
}
