use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::{info, warn};

use zihuan_agent::brain::BrainIterationHook;
use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_agent::utils::build_state_system_prefix_lines;

use zihuan_core::agent_config::QqChatEmotionDimensionConfig;
use zihuan_core::error::Result;
use zihuan_core::llm::{LLMMessage, MessagePart};
use zihuan_core::steer::{
    apply_steer_prefix, build_merged_follow_up_event, PendingSteerEvent, PendingSteerStore, PROCESSING_INSTRUCTION,
};
use zihuan_core::utils::string_utils::shorten_text;

use zihuan_graph_engine::brain_tool_spec::QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::DataValue;

use ims_bot_adapter::message_helpers::get_bot_id;
use ims_bot_adapter::{CURRENT_MESSAGE_LABEL, IMAGE_ANALYSIS_LABEL};

use crate::qq_chat_user_input::{
    append_prepared_parts, build_prepared_input_metadata, expand_messages_for_inference, flush_text_part,
    prepare_current_turn_user_input, prepare_current_turn_user_input_from_event, PreparedCurrentTurnUserInput,
};
use crate::storage::qq_chat_history_store::{conversation_history_key, load_history};

use super::qq_chat_agent_service_core::{
    build_state_delta_lines, build_user_message, QqChatAgentServiceContext, QqChatAgentServiceInner,
    QqChatServiceHandleReport, LOG_PREFIX, LOG_TEXT_PREVIEW_CHARS,
};
use super::qq_chat_agent_service_logging::QqChatTaskTrace;

const CURRENT_USER_MESSAGE_LABEL: &str = "[Current User Message]";
const REFERENCED_CONTEXT_LABEL: &str = "[Referenced Context]";
const REFERENCE_ONLY_NOTICE: &str =
    "The following content is reference only. Do not automatically treat it as the current sender's own statement.";

fn build_steer_user_message(
    current_input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    adapter: &ims_bot_adapter::adapter::SharedBotAdapter,
    llm_supports_multimodal_input: bool,
    api_style: Option<&str>,
    system_prompt: &str,
    session_state: &mut QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> LLMMessage {
    let steer_message = build_user_message(
        current_input,
        bot_name,
        adapter,
        llm_supports_multimodal_input,
        system_prompt,
        session_state,
        emotion_dimensions,
    );

    apply_steer_prefix(steer_message, api_style)
}

fn build_merged_steer_user_message(
    current_inputs: &[PreparedCurrentTurnUserInput],
    bot_name: &str,
    adapter: &ims_bot_adapter::adapter::SharedBotAdapter,
    llm_supports_multimodal_input: bool,
    api_style: Option<&str>,
    system_prompt: &str,
    session_state: &mut QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> LLMMessage {
    let prefix_lines = build_state_system_prefix_lines(session_state, emotion_dimensions, system_prompt);
    let prefix = prefix_lines.join("\n");
    let mut state_delta_lines = Vec::new();
    if let Some(last_input) = current_inputs.last() {
        state_delta_lines = build_state_delta_lines(session_state, last_input, bot_name, adapter, emotion_dimensions);
    }
    let state_delta_block = if state_delta_lines.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", state_delta_lines.join("\n"))
    };

    let build_entry_text = |index: usize, input: &PreparedCurrentTurnUserInput| -> String {
        let metadata_text = build_prepared_input_metadata(input, bot_name);
        let current_image_section = if input.current_image_reference_lines.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n[{}]\n{}",
                IMAGE_ANALYSIS_LABEL,
                input.current_image_reference_lines.join("\n")
            )
        };
        let referenced_context = if input.has_reference_context() {
            let reference_image_section = if input.reference_image_reference_lines.is_empty() {
                String::new()
            } else {
                format!(
                    "\n\n[{}]\n{}",
                    IMAGE_ANALYSIS_LABEL,
                    input.reference_image_reference_lines.join("\n")
                )
            };
            format!(
                "\n\n{REFERENCED_CONTEXT_LABEL}\n{REFERENCE_ONLY_NOTICE}\n{}{reference_image_section}",
                input.referenced_context_text()
            )
        } else {
            String::new()
        };
        format!(
            "{}. {metadata_text}\n{CURRENT_USER_MESSAGE_LABEL}\n{input_text}{current_image_section}{referenced_context}",
            index + 1,
            input_text = input.current_text_for_prompt(),
        )
    };

    if !llm_supports_multimodal_input {
        let merged_text = current_inputs
            .iter()
            .enumerate()
            .map(|(index, input)| build_entry_text(index, input))
            .collect::<Vec<_>>()
            .join("\n\n");

        let message = LLMMessage::user(format!(
            "{prefix}\n\n{state_delta_block}{merged_text}\n\n{PROCESSING_INSTRUCTION}"
        ));
        return apply_steer_prefix(message, api_style);
    }

    let state_text = format!("{}\n", prefix_lines.join("\n"));
    let mut parts = vec![MessagePart::text(state_text.clone())];
    let mut text_buffer = state_delta_block;

    for (index, current_input) in current_inputs.iter().enumerate() {
        if index > 0 {
            text_buffer.push_str("\n\n");
        }
        let metadata_text = build_prepared_input_metadata(current_input, bot_name);
        text_buffer.push_str(&format!(
            "{}. {metadata_text}\n{CURRENT_USER_MESSAGE_LABEL}\n{}",
            index + 1,
            current_input.current_text_for_prompt()
        ));
        append_prepared_parts(&mut parts, &mut text_buffer, "\n", &current_input.current_parts);
        if !current_input.current_image_reference_lines.is_empty() {
            text_buffer.push_str(&format!(
                "\n\n[{}]\n{}",
                IMAGE_ANALYSIS_LABEL,
                current_input.current_image_reference_lines.join("\n")
            ));
        }
        if current_input.has_reference_context() {
            text_buffer.push_str(&format!("\n\n{REFERENCED_CONTEXT_LABEL}\n{REFERENCE_ONLY_NOTICE}"));
            let reference_text = current_input.referenced_context_text();
            if !reference_text.trim().is_empty() {
                text_buffer.push('\n');
                text_buffer.push_str(reference_text.trim());
            }
            append_prepared_parts(&mut parts, &mut text_buffer, "\n", &current_input.reference_parts);
            if !current_input.reference_image_reference_lines.is_empty() {
                text_buffer.push_str(&format!(
                    "\n\n[{}]\n{}",
                    IMAGE_ANALYSIS_LABEL,
                    current_input.reference_image_reference_lines.join("\n")
                ));
            }
        }
    }

    flush_text_part(&mut parts, &mut text_buffer);
    let has_media = current_inputs.iter().any(|input| input.has_media);

    let message = if has_media && parts.len() > 1 {
        parts.push(MessagePart::text(PROCESSING_INSTRUCTION.to_string()));
        LLMMessage::user_with_parts(parts)
    } else {
        let merged_text = current_inputs
            .iter()
            .enumerate()
            .map(|(index, input)| build_entry_text(index, input))
            .collect::<Vec<_>>()
            .join("\n\n");
        LLMMessage::user(format!("{state_text}\n{merged_text}\n\n{PROCESSING_INSTRUCTION}"))
    };

    apply_steer_prefix(message, api_style)
}

pub(crate) struct QqChatServiceSteerHook {
    pub(crate) pending_steer: Arc<PendingSteerStore>,
    pub(crate) sender_id: String,
    pub(crate) bot_id: String,
    pub(crate) bot_name: String,
    pub(crate) adapter: ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) max_steer_count: usize,
    pub(crate) llm_supports_multimodal_input: bool,
    pub(crate) llm_api_style: Option<String>,
    pub(crate) s3_ref: Option<Arc<S3Ref>>,
    pub(crate) trace: QqChatTaskTrace,
    pub(crate) consumed_messages: Arc<Mutex<Vec<LLMMessage>>>,
    pub(crate) shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
    pub(crate) system_prompt: String,
    pub(crate) session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
}

impl BrainIterationHook for QqChatServiceSteerHook {
    fn on_before_inference(&self, _iteration: usize, _conversation: &[LLMMessage]) -> Vec<LLMMessage> {
        let (pending, remaining_queue_len, accepted_steer_count) = self.pending_steer.drain_all(&self.sender_id);
        if pending.is_empty() {
            return Vec::new();
        }
        let steer_count = pending.len();

        let mut injected = Vec::with_capacity(pending.len());
        let mut prepared_inputs = Vec::with_capacity(pending.len());
        let mut consumed_guard = self.consumed_messages.lock().unwrap();

        for pending_event in pending {
            let mut inference_event = pending_event.event.clone();
            inference_event.message_list = expand_messages_for_inference(&pending_event.event.message_list);
            let prepared_input = prepare_current_turn_user_input_from_event(
                &inference_event,
                &self.bot_id,
                &self.bot_name,
                self.s3_ref.as_ref(),
            );
            let current_message = prepared_input.current_text_for_prompt().to_string();
            self.trace.record_steer_received(&current_message);
            prepared_inputs.push(prepared_input);
            injected.push(inference_event);
        }

        let steer_message = if prepared_inputs.len() == 1 {
            let mut session_state = self.session_state.lock().unwrap();
            build_steer_user_message(
                &prepared_inputs[0],
                &self.bot_name,
                &self.adapter,
                self.llm_supports_multimodal_input,
                self.llm_api_style.as_deref(),
                &self.system_prompt,
                &mut session_state,
                &self.emotion_dimensions,
            )
        } else {
            let mut session_state = self.session_state.lock().unwrap();
            build_merged_steer_user_message(
                &prepared_inputs,
                &self.bot_name,
                &self.adapter,
                self.llm_supports_multimodal_input,
                self.llm_api_style.as_deref(),
                &self.system_prompt,
                &mut session_state,
                &self.emotion_dimensions,
            )
        };
        consumed_guard.push(steer_message.clone());
        drop(consumed_guard);
        self.trace.record_steer_injected(
            steer_count,
            1,
            accepted_steer_count,
            self.max_steer_count,
            remaining_queue_len,
            std::slice::from_ref(&steer_message),
        );
        {
            let last_injected = injected.last().expect("injected must be non-empty");
            let mut shared_rt = self.shared_runtime_values.lock().unwrap();
            shared_rt.insert(
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
                DataValue::MessageEvent(last_injected.clone()),
            );
        }
        vec![steer_message]
    }
}

impl QqChatAgentServiceInner {
    pub(crate) fn try_handle_busy_session_steer(
        &self,
        event: &ims_bot_adapter::models::MessageEvent,
        ctx: &QqChatAgentServiceContext<'_>,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        time: &str,
    ) -> Result<()> {
        let bot_id = get_bot_id(ctx.adapter);
        let prepared_input = prepare_current_turn_user_input(event, ctx.adapter, &bot_id, ctx.bot_name, ctx.s3_ref);
        let mut inference_event = prepared_input.event.clone();
        inference_event.message_list = expand_messages_for_inference(&prepared_input.event.message_list);
        let current_message =
            prepare_current_turn_user_input_from_event(&inference_event, &bot_id, ctx.bot_name, ctx.s3_ref)
                .current_text_for_prompt()
                .to_string();
        if let Some(command_registry) = crate::command::global_command_registry() {
            let cmd_ctx = self.build_command_context(sender_id, target_id, is_group, inference_event.group_id);
            if let Some(preview) = command_registry.preview(&cmd_ctx, &current_message) {
                if preview.definition.allow_steer_bypass && preview.passthrough_text.is_none() {
                    info!(
                        "{LOG_PREFIX} Session busy for {sender_id}, executing command via steer bypass: message_id={} command=/{}",
                        event.message_id,
                        preview.definition.name
                    );
                    if let Some(dispatch_result) = command_registry.dispatch(&cmd_ctx, &current_message) {
                        let history_key =
                            conversation_history_key(&bot_id, sender_id, is_group, inference_event.group_id);
                        let legacy_history_key = sender_id.to_string();
                        let mut history = load_history(ctx.cache, &history_key, &legacy_history_key);
                        let trace = QqChatTaskTrace::new(Local::now());
                        self.execute_command_dispatch(
                            &trace,
                            &cmd_ctx,
                            dispatch_result,
                            &prepared_input.event,
                            &inference_event,
                            sender_id,
                            target_id,
                            &bot_id,
                            &mut history,
                            ctx,
                        )?;
                        trace.finish_with_summary();
                        return Ok(());
                    }
                } else {
                    info!(
                        "{LOG_PREFIX} Session busy for {sender_id}, command falls back to steer: message_id={} command=/{} allow_steer_bypass={} has_passthrough={}",
                        event.message_id,
                        preview.definition.name,
                        preview.definition.allow_steer_bypass,
                        preview.passthrough_text.is_some()
                    );
                }
            }
        }
        let (accepted, queue_len, accepted_steer_count) = ctx.pending_steer.enqueue_with_limit(
            sender_id,
            PendingSteerEvent {
                event: prepared_input.event,
                time: time.to_string(),
            },
            ctx.max_steer_count,
        );
        if accepted {
            info!(
                "{LOG_PREFIX} Session busy for {sender_id}, enqueueing steer: message_id={} queue_len={} accepted_steer_count={}/{} message={}",
                event.message_id,
                queue_len,
                accepted_steer_count,
                ctx.max_steer_count,
                shorten_text(&current_message, LOG_TEXT_PREVIEW_CHARS)
            );
        } else {
            warn!(
                "{LOG_PREFIX} steer dropped for sender={} message_id={} because max steer count reached: accepted_steer_count={}/{} message={}",
                sender_id,
                event.message_id,
                accepted_steer_count,
                ctx.max_steer_count,
                shorten_text(&current_message, LOG_TEXT_PREVIEW_CHARS)
            );
        }
        Ok(())
    }
}

impl QqChatAgentServiceInner {
    pub(crate) fn handle_claimed(
        &self,
        trace: &QqChatTaskTrace,
        event: &ims_bot_adapter::models::MessageEvent,
        time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        ctx: &QqChatAgentServiceContext<'_>,
    ) -> Result<QqChatServiceHandleReport> {
        (|| -> Result<QqChatServiceHandleReport> {
            let bot_id = get_bot_id(ctx.adapter);
            let mut current_event = event.clone();
            let mut current_time = time.to_string();
            let result_summary = loop {
                let turn_result = self.handle_claimed_turn(
                    trace,
                    &current_event,
                    &current_time,
                    sender_id,
                    target_id,
                    is_group,
                    &bot_id,
                    ctx,
                )?;

                let (pending, remaining_queue_len, accepted_steer_count) = ctx.pending_steer.drain_all(sender_id);
                if pending.is_empty() {
                    break turn_result.result_summary;
                }

                let steer_count = pending.len();
                let next_event = build_merged_follow_up_event(&pending);
                let mut next_inference_event = next_event.clone();
                next_inference_event.message_list = expand_messages_for_inference(&next_event.message_list);
                let next_message = prepare_current_turn_user_input_from_event(
                    &next_inference_event,
                    &bot_id,
                    ctx.bot_name,
                    ctx.s3_ref,
                )
                .current_text_for_prompt()
                .to_string();
                trace.record_steer_follow_up(
                    next_event.message_id,
                    steer_count,
                    accepted_steer_count,
                    ctx.max_steer_count,
                    &next_message,
                );
                info!(
                    "{LOG_PREFIX} steer follow-up picked for sender={} message_id={} steer_count={} remaining_queue_len={} accepted_steer_count={}/{} message={}",
                    sender_id,
                    next_event.message_id,
                    steer_count,
                    remaining_queue_len,
                    accepted_steer_count,
                    ctx.max_steer_count,
                    shorten_text(&next_message, LOG_TEXT_PREVIEW_CHARS)
                );
                current_event = next_event;
                current_time = pending
                    .last()
                    .map(|event| event.time.clone())
                    .unwrap_or_else(|| current_time.clone());
            };

            Ok(QqChatServiceHandleReport { result_summary })
        })()
    }
}
