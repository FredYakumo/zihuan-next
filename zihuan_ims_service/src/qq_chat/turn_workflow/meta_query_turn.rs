//! The meta-query path of one turn: intents like asking for the tool list, the system
//! prompt, or the model name are answered directly with a dedicated LLM context
//! (pre-fetched function list and public info) without exposing any tool specs.
//!
//! The LLM receives only safe, pre-fetched data as text context and is instructed to
//! rephrase it into natural language. The result still goes through the after-brain
//! review as a safety net. Chain (documents/procedure.md): meta-query brain (a single
//! direct inference) -> after-brain -> format-output.

use std::sync::{Arc, Mutex};

use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole};
use zihuan_core::qq_context::{
    emit_qq_context_event, emit_qq_context_stage, QqChatContextEvent,
    QQ_CONTEXT_STAGE_SYSTEM_PROMPT,
};
use zihuan_core::role::procedure::execute_blocking_procedure_chain;
use zihuan_core::runtime::block_async;
use zihuan_core::steer::message_with_api_style;
use zihuan_core::utils::string_utils::shorten_text;

use super::super::core::{
    build_meta_query_system_prompt, build_meta_query_user_message, compact_turn_history,
    QqChatAgentServiceContext, QqChatServiceTurnResult,
};
use super::finalization::{
    deliver_reply, rearm_jobs_if_replied, record_rethink_stage, write_back_session_state,
};
use super::{build_after_brain, build_format_output, TurnContext};
use crate::agent::emotion::utils::{emotion_expression_prompt, has_noticeable_emotion_expression};
use crate::procedure::{
    qq_procedure_context, QqBrainOutput, QqFormatOutputOutput, QqMetaQueryBrain,
    QqReplyReviewResult,
};
use crate::storage::qq_chat_history_store::save_history;
use crate::tools::format_public_info_message;

/// Handles meta-query intents by using a dedicated LLM context with pre-fetched function
/// list and public info, without exposing any tool specs to the LLM.
pub(crate) fn run(turn: TurnContext<'_>) -> Result<QqChatServiceTurnResult> {
    let turn_id = turn.turn_id;
    let sender_id = turn.sender_id;
    let ctx = turn.ctx;
    let trace = turn.trace;
    let is_group = turn.is_group;

    // The caller passes the raw loaded history; compact it here with a plain user message
    // as the token-estimate reference so the persisted history stays within budget too.
    let mut history = {
        let reference = message_with_api_style(
            LLMMessage::user(turn.current_message.to_string()),
            ctx.llm.api_style(),
        );
        compact_turn_history(ctx.cache, &turn.history_key, ctx.llm, turn.history, &reference)
            .messages
    };
    let function_list =
        zihuan_core::command::build_help_text().unwrap_or_else(|| "暂无可用功能信息。".to_string());
    let public_info = format_public_info_message(&turn.current_message).to_string();

    let style_prompt = ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str());
    let (emotion_prompt, suppress_language_style) = {
        let session = turn.turn_session_state.lock().unwrap();
        (
            emotion_expression_prompt(&session, &turn.emotion_dimensions),
            has_noticeable_emotion_expression(&session, &turn.emotion_dimensions),
        )
    };
    let style_prompt = if suppress_language_style {
        None
    } else {
        style_prompt
    };

    let meta_system_prompt =
        build_meta_query_system_prompt(ctx.bot_name, style_prompt, &emotion_prompt);
    let meta_user_message =
        build_meta_query_user_message(&turn.current_message, &function_list, &public_info);

    let meta_messages =
        vec![LLMMessage::system(meta_system_prompt), LLMMessage::user(meta_user_message)];
    emit_qq_context_stage(
        ctx.agent_id,
        sender_id,
        turn_id,
        QQ_CONTEXT_STAGE_SYSTEM_PROMPT,
        serde_json::json!({ "prompt": meta_messages[0].content_text_owned() }),
    );

    // Role turn chain (documents/procedure.md): the meta-query brain (a single direct
    // inference) -> AfterBrain review -> FormatOutput.
    let mut shared = qq_procedure_context(&turn.history_key, None);
    // Same guard hazard as the normal turn: clone outside the `block_async` arguments.
    let after_brain_session_state = turn.turn_session_state.lock().unwrap().clone();
    let shared_runtime_values = Arc::new(Mutex::new(ctx.shared_runtime_values.clone()));
    let meta_outputs = block_async(execute_blocking_procedure_chain(
        vec![
            Arc::new(QqMetaQueryBrain::new(ctx.llm, trace, meta_messages)),
            Arc::new(build_after_brain(
                ctx,
                trace,
                is_group,
                sender_id,
                &turn.inference_event,
                after_brain_session_state,
                turn.emotion_dimensions.clone(),
            )),
            Arc::new(build_format_output(
                ctx,
                trace,
                turn_id,
                is_group,
                sender_id,
                turn.bot_id,
                &turn.inference_event,
                shared_runtime_values,
            )),
        ],
        &mut shared,
    ))?;
    let meta_payload = shared
        .find_output::<QqBrainOutput>()
        .map(|value| (*value).clone())
        .ok_or_else(|| {
            Error::ValidationError(
                "qq meta-query brain procedure produced no turn output".to_string(),
            )
        })?;
    let Some(candidate_message) = meta_payload.final_reply_text.as_ref() else {
        emit_meta_query_turn_finished(ctx, sender_id, turn_id, &meta_payload, None);
        return Ok(QqChatServiceTurnResult {
            result_summary: format!("元查询[{sender_id}]：LLM未返回有效回复"),
            turn_completed: true,
        });
    };
    emit_qq_context_stage(
        ctx.agent_id,
        sender_id,
        turn_id,
        "元查询直答",
        serde_json::json!({
            "model": ctx.llm.get_model_name(),
            "tools": 0,
            "answer": candidate_message,
        }),
    );

    if meta_payload.suppress_send {
        history.push(message_with_api_style(
            LLMMessage::user(turn.current_message.to_string()),
            ctx.llm.api_style(),
        ));
        save_history(ctx.cache, &turn.history_key, history);
        write_back_session_state(ctx, &turn.turn_session_state);
        trace.record_final_reply_decision(Some(candidate_message), Some(true), None);
        emit_meta_query_turn_finished(ctx, sender_id, turn_id, &meta_payload, None);
        return Ok(QqChatServiceTurnResult {
            result_summary: format!("已处理[{sender_id}]的元查询，显式选择不回复"),
            turn_completed: true,
        });
    }

    let review_result = meta_outputs
        .iter()
        .rev()
        .find_map(|output| output.cloned::<QqReplyReviewResult>())
        .ok_or_else(|| {
            Error::ValidationError("after-brain procedure produced no review result".to_string())
        })?;
    record_rethink_stage(ctx, sender_id, turn_id, Some(candidate_message), &review_result);
    let format_output = meta_outputs
        .iter()
        .rev()
        .find_map(|output| output.cloned::<QqFormatOutputOutput>())
        .ok_or_else(|| {
            Error::ValidationError("format-output procedure produced no turn output".to_string())
        })?;

    let (_sent_batches, _explicit_no_reply, visible_assistant_history_text) = deliver_reply(
        trace,
        ctx,
        turn.event,
        sender_id,
        turn_id,
        turn.target_id,
        is_group,
        turn.bot_id,
        &format_output,
    );

    history.push(message_with_api_style(
        LLMMessage::user(turn.current_message.to_string()),
        ctx.llm.api_style(),
    ));
    if let Some(ref assistant_text) = visible_assistant_history_text {
        history.push(message_with_api_style(
            LLMMessage::assistant_text(assistant_text.clone()),
            ctx.llm.api_style(),
        ));
    }
    save_history(ctx.cache, &turn.history_key, history);
    rearm_jobs_if_replied(ctx, sender_id, visible_assistant_history_text.is_some());
    write_back_session_state(ctx, &turn.turn_session_state);

    let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
        format!("已回复[{sender_id}]的元查询，内容：{}", shorten_text(assistant_text, 80))
    } else {
        format!("已处理[{sender_id}]的元查询，但未发送回复")
    };
    trace.log_result_summary(&result_summary);
    emit_meta_query_turn_finished(
        ctx,
        sender_id,
        turn_id,
        &meta_payload,
        visible_assistant_history_text.as_deref(),
    );

    Ok(QqChatServiceTurnResult { result_summary, turn_completed: true })
}

/// Close the context-view turn for a meta-query path, which skips the brain loop.
/// Unlike the normal path it carries no generation timing — there is no tool loop.
fn emit_meta_query_turn_finished(
    ctx: &QqChatAgentServiceContext<'_>,
    sender_id: &str,
    turn_id: &str,
    payload: &QqBrainOutput,
    assistant_text: Option<&str>,
) {
    let last_assistant = payload
        .brain_output
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Assistant));
    let usage = last_assistant.and_then(|message| message.usage.clone());
    let reasoning_text = last_assistant
        .and_then(|message| message.reasoning_content.clone())
        .filter(|text| !text.trim().is_empty());
    emit_qq_context_event(QqChatContextEvent::TurnFinished {
        agent_id: ctx.agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        assistant_text: assistant_text.map(ToOwned::to_owned),
        reasoning_text,
        usage,
        generation_ms: None,
        output_tokens_per_second: None,
    });
}
