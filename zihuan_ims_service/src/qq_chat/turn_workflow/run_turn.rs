//! The turn orchestrator: one claimed turn, prepared and dispatched.
//!
//! [`run_turn`] owns the lifecycle every turn shares — input hydration, command
//! interception, intent classification — and then dispatches to the meta-query or the
//! normal brain path with the assembled [`TurnContext`]. Which path a turn takes is
//! decided by the intent classification (documents/procedure.md: which procedures a turn
//! uses is decided per event).

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use log::info;

use zihuan_core::error::Result;
use zihuan_core::qq_context::{
    collect_turn_media_refs, emit_qq_context_event, emit_qq_context_stage, QqChatContextEvent,
    TurnTraceGuard,
};

use super::super::core::{
    expand_messages_for_inference, prepare_current_turn_user_input,
    prepare_current_turn_user_input_from_event, QqChatAgentService, QqChatServiceTurnResult,
    LOG_PREFIX,
};
use super::super::logging::emit_context_turn_failure;
use crate::classify_intent::{classify_intent_with_trace, IntentCategory};
use crate::qq_chat::command::{run_command_pipeline, CommandTurnEnd};
use crate::qq_chat::resources::current_qq_chat_role_service_config;
use crate::storage::qq_chat_history_store::{conversation_history_key, load_history};

use super::{rate_limit_stage_detail, selected_turn_llm, TurnContext, TurnInput};

/// Processes a single QQ chat turn end-to-end for a claimed message.
///
/// The lifecycle is:
/// - **Hydration & extraction** — resolves reply chains and extracts the user text.
/// - **Command interception** — runs the message through the QQ command pipeline
///   (`crate::qq_chat::command`), which either consumes the turn or leaves
///   remaining text to the brain loop.
/// - **Intent classification** — selects the turn's chain composition and the routed LLM
///   (general vs math/programming).
/// - **Short-circuit replies** — answers meta-queries (model name, tool list, etc.) directly.
/// - **Role turn chain** — one procedure chain (documents/procedure.md): before-brain
///   (assembles the system prompt and user input, including memory titles and the emotion
///   prompt) -> brain (compacts history, runs the tool-calling loop with steer support) ->
///   after-brain (reply review/rewrite).
/// - **Reply delivery** — parses the final assistant output and sends it back to the user
///   (group or private chat), persisting message history along the way.
///
/// Returns a [`QqChatServiceTurnResult`] containing a human-readable summary of what happened.
pub(crate) fn run_turn(
    service: &QqChatAgentService,
    input: &TurnInput<'_>,
) -> Result<QqChatServiceTurnResult> {
    let message_rate_limit_warning =
        input.message_rate_limit.and_then(|result| result.warning_prompt.as_deref());
    let prepared_input = prepare_current_turn_user_input(
        input.event,
        input.ctx.adapter,
        input.bot_id,
        input.ctx.bot_name,
        input.ctx.s3_ref,
    );
    let mut inference_event = prepared_input.event.clone();
    inference_event.message_list =
        expand_messages_for_inference(&prepared_input.event.message_list);
    let inference_input = prepare_current_turn_user_input_from_event(
        &inference_event,
        input.bot_id,
        input.ctx.bot_name,
        input.ctx.s3_ref,
    );
    let raw_user_message = prepared_input.current_text_for_prompt().to_string();
    let mut current_message = inference_input.current_text_for_prompt().to_string();
    input.trace.log_user_message(&raw_user_message, &current_message);

    // Identity for this turn's live context events (Data Explorer view).
    let turn_id = uuid::Uuid::new_v4().to_string();
    // Intent classification and before-brain retrieval run synchronously on this blocking
    // turn thread, so binding the turn here lets the embedding runtime and the usage-tracked
    // LLM wrapper report each inference as a live context stage without threading the turn
    // identity through every call.
    let _turn_trace = TurnTraceGuard::enter(input.ctx.agent_id, input.sender_id, &turn_id);
    let turn_media = collect_turn_media_refs(&prepared_input.event.message_list);

    let role_config = current_qq_chat_role_service_config()?;
    let emotion_dimensions = role_config.resolved_emotion_dimensions();
    let now_unix_seconds =
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    {
        let mut session_state = input.ctx.session_state_store.lock().unwrap();
        session_state.dissipate_expired_emotions(&emotion_dimensions, now_unix_seconds);
        session_state.record_conversation_activity(now_unix_seconds);
    }

    // Reset session-level tool quota for each new turn so that
    // tool call limits apply per single user message cycle and not
    // across the entire agent service lifetime.
    if let Some(ref quota) = input.ctx.tool_quota {
        quota.session_state.lock().unwrap().reset();
    }

    let history_key = conversation_history_key(input.sender_id);
    let history = load_history(input.ctx.cache, &history_key);

    // Command interception runs through the unified command state machine
    // (crate::qq_chat::command). The pipeline resolves the spec, runs
    // preconditions/body steps via the QQ channel runtime, and returns
    // effects (or a pause) for this turn.
    match run_command_pipeline(
        &service.id,
        input.trace,
        input.event,
        &inference_event,
        &raw_user_message,
        input.sender_id,
        input.target_id,
        input.bot_id,
        input.is_group,
        &emotion_dimensions,
        input.ctx,
    )? {
        CommandTurnEnd::Consumed(result) => return Ok(result),
        CommandTurnEnd::Passthrough(text) => current_message = text,
        CommandTurnEnd::NotACommand => {}
    }

    // Non-command messages fall through to the brain loop.

    let mut current_session_state = input.ctx.session_state_store.lock().unwrap().clone();
    current_session_state.sync_emotion_dimensions(&emotion_dimensions);
    let turn_session_state = Arc::new(Mutex::new(current_session_state));

    // Announce the turn before the first real step runs, so every stage and tool call that
    // follows is appended in the order it actually happened.
    emit_qq_context_event(QqChatContextEvent::TurnStarted {
        agent_id: input.ctx.agent_id.to_string(),
        sender_id: input.sender_id.to_string(),
        sender_name: zihuan_core::ims_bot_adapter::utils::sender_display_name!(
            &input.event.sender.nickname,
            &input.event.sender.card
        ),
        group_id: input.event.group_id.map(|value| value.to_string()),
        group_name: input.event.group_name.clone(),
        is_group: input.is_group,
        turn_id: turn_id.clone(),
        created_at: chrono::Local::now().to_rfc3339(),
        user_text: raw_user_message.clone(),
        quoted: prepared_input.quoted_message.clone(),
        media: turn_media.clone(),
        continues_previous: input.continues_previous,
    });

    // Report the rate-limit decision once per turn so the context view can show which
    // quota applied and how much of it had been consumed. Steer follow-ups share the
    // original turn's block and would otherwise repeat the same snapshot.
    if !input.continues_previous {
        emit_qq_context_stage(
            input.ctx.agent_id,
            input.sender_id,
            &turn_id,
            "限流",
            rate_limit_stage_detail(input.message_rate_limit),
        );
    }

    // Intent classification picks the turn's chain composition (meta-query vs brain) and
    // the routed brain model, so it runs before any procedure chain is assembled
    // (documents/procedure.md: which procedures a turn uses is decided per event). It is
    // the preprompt phase's model call: the "Preprompt 模型" setting, falling back to the
    // main model when unset.
    let intent_trace = classify_intent_with_trace(
        input.ctx.intent_classification_llm,
        input.ctx.embedding_model,
        &current_message,
        Some(&history),
        800,
    );
    let (turn_llm, routed_model) = selected_turn_llm(input.ctx, intent_trace.category);
    input.trace.record_intent_classification(&intent_trace, routed_model);
    emit_qq_context_stage(
        input.ctx.agent_id,
        input.sender_id,
        &turn_id,
        "意图识别",
        serde_json::json!({
            "category": intent_trace.category.label(),
            "path": intent_trace.path.label(),
            "used_embedding": intent_trace.used_embedding,
            "used_llm": intent_trace.used_llm,
            "embedding_duration_ms": intent_trace.embedding_duration_ms,
            "duration_ms": intent_trace.total_duration_ms,
            "raw_label": intent_trace.raw_label,
            "routed_model": routed_model,
            "model": turn_llm.get_model_name(),
        }),
    );

    // Only adversarial leak probes (tool list, system prompt) short-circuit to the meta-query
    // chain: it answers directly with pre-fetched safe data and has no history or memory access.
    // Model/identity questions are legitimate conversation, so they run the normal chain where
    // the brain agent can answer from `get_agent_public_info` with context and memory, and the
    // after-brain polishes the reply.
    let is_meta_query = matches!(
        intent_trace.category,
        IntentCategory::AskToolList | IntentCategory::AskSystemPrompt
    );

    let turn_context = TurnContext {
        service,
        trace: input.trace,
        ctx: input.ctx,
        event: input.event,
        sender_id: input.sender_id,
        target_id: input.target_id,
        is_group: input.is_group,
        bot_id: input.bot_id,
        continues_previous: input.continues_previous,
        message_rate_limit_warning,
        turn_id: &turn_id,
        prepared_input,
        inference_event,
        current_message,
        history_key,
        history,
        turn_llm,
        role_config,
        emotion_dimensions,
        turn_session_state,
    };

    if is_meta_query {
        info!(
            "{LOG_PREFIX} meta-query short-circuit for sender={}, intent={}",
            input.sender_id,
            intent_trace.category.label()
        );
        // Meta-query turns answer directly without the brain loop. The turn was already
        // announced, so that path closes it with its own `TurnFinished` event.
        let meta_result = super::meta_query_turn::run(turn_context);
        if let Err(ref error) = &meta_result {
            emit_context_turn_failure(
                input.ctx.agent_id,
                input.sender_id,
                &turn_id,
                "元查询",
                error,
            );
        }
        return meta_result;
    }

    // Normal turn: one procedure chain, before-brain -> brain -> after-brain
    // (documents/procedure.md). The before-brain procedure assembles the turn's system
    // prompt and user input; the brain reads them through the shared procedure context.
    super::normal_turn::run(turn_context)
}
