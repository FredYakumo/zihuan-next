use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Local;
use log::{info, warn};

use crate::agent::emotion::utils::emotion_expression_prompt;
use crate::agent::utils::build_turn_state_prefix_lines;
use crate::qq_session_state::QqChatSessionState;
use zihuan_core::agent::tools::memory::MemoryRecall;
use zihuan_core::agent::tools::ToolCallingMiddleware;
use zihuan_core::command::{CommandChannel, CommandContext};

use crate::role_config::QqChatEmotionDimensionConfig;
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::{LLMMessage, MessagePart};
use zihuan_core::steer::{
    apply_steer_prefix, PendingSteerEvent, PendingSteerStore, STEER_PREFIX,
    STEER_PROCESSING_INSTRUCTION,
};
use zihuan_core::utils::string_utils::shorten_text;

use zihuan_core::graph::tool_spec::QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT;
use zihuan_core::graph::DataValue;
use zihuan_core::storage::S3Ref;

use zihuan_core::ims_bot_adapter::message_helpers::get_bot_id;
use zihuan_core::ims_bot_adapter::IMAGE_ANALYSIS_LABEL;

use super::user_input::{
    append_prepared_parts, build_prepared_input_metadata, expand_messages_for_inference,
    flush_text_part, prepare_current_turn_user_input, prepare_current_turn_user_input_from_event,
    PreparedCurrentTurnUserInput,
};

use super::core::{
    build_state_delta_lines, build_user_message, QqChatAgentService, QqChatAgentServiceContext,
    LOG_PREFIX, LOG_TEXT_PREVIEW_CHARS,
};
use super::logging::QqChatTaskTrace;

const CURRENT_USER_MESSAGE_LABEL: &str = "[Current User Message]";
const REFERENCED_CONTEXT_LABEL: &str = "[Referenced Context]";
const REFERENCE_ONLY_NOTICE: &str =
    "The following content is reference only. Do not automatically treat it as the current sender's own statement.";
/// The steer variant of the normal turn's reply demand: it states the message arrived
/// mid-task instead of demanding a reply to the steer alone.
const STEER_REPLY_REQUEST: &str =
    "This message arrived while you were already working on the current task.";

/// A steer message is an extra user turn injected into the running loop, so it carries the same
/// turn state prefix (emotion prompt, recalled memories) as the turn it interrupts. The stable
/// rules live in the conversation's system message and are never repeated here.
///
/// Unlike a normal turn it does not demand a reply: the reply request and the processing
/// instructions state that the message arrived mid-task and the current task comes first.
/// The steer prefix (and any conversation-switch notice) is applied by the hook, not here.
fn build_steer_user_message(
    current_input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    adapter: &zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter,
    llm_supports_multimodal_input: bool,
    session_state: &mut QqChatSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    memory_recall: Option<&MemoryRecall>,
) -> LLMMessage {
    build_user_message(
        current_input,
        bot_name,
        adapter,
        llm_supports_multimodal_input,
        None,
        session_state,
        emotion_dimensions,
        memory_recall,
        None,
        STEER_REPLY_REQUEST,
        STEER_PROCESSING_INSTRUCTION,
    )
}

fn build_merged_steer_user_message(
    current_inputs: &[PreparedCurrentTurnUserInput],
    bot_name: &str,
    adapter: &zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter,
    llm_supports_multimodal_input: bool,
    session_state: &mut QqChatSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    memory_recall: Option<&MemoryRecall>,
) -> LLMMessage {
    let emotion_prompt = emotion_expression_prompt(session_state, emotion_dimensions);
    let prefix_lines = build_turn_state_prefix_lines(&emotion_prompt, memory_recall);
    let prefix = prefix_lines.join("\n");
    let mut state_delta_lines = Vec::new();
    if let Some(last_input) = current_inputs.last() {
        state_delta_lines = build_state_delta_lines(
            session_state,
            last_input,
            bot_name,
            adapter,
            emotion_dimensions,
        );
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

        let state_block = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}\n\n")
        };
        let message = LLMMessage::user(format!(
            "{state_block}{state_delta_block}{merged_text}\n\n{STEER_PROCESSING_INSTRUCTION}"
        ));
        return message;
    }

    let state_text = if prefix.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", prefix_lines.join("\n"))
    };
    let mut parts: Vec<MessagePart> = Vec::new();
    let mut text_buffer = format!("{state_text}{state_delta_block}");

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
            text_buffer
                .push_str(&format!("\n\n{REFERENCED_CONTEXT_LABEL}\n{REFERENCE_ONLY_NOTICE}"));
            let reference_text = current_input.referenced_context_text();
            if !reference_text.trim().is_empty() {
                text_buffer.push('\n');
                text_buffer.push_str(reference_text.trim());
            }
            append_prepared_parts(
                &mut parts,
                &mut text_buffer,
                "\n",
                &current_input.reference_parts,
            );
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
    let has_media_part = parts.iter().any(|part| !matches!(part, MessagePart::Text { .. }));

    let message = if has_media_part {
        parts.push(MessagePart::text(STEER_PROCESSING_INSTRUCTION.to_string()));
        LLMMessage::user_with_parts(parts)
    } else {
        let merged_text = current_inputs
            .iter()
            .enumerate()
            .map(|(index, input)| build_entry_text(index, input))
            .collect::<Vec<_>>()
            .join("\n\n");
        LLMMessage::user(format!("{state_text}{merged_text}\n\n{STEER_PROCESSING_INSTRUCTION}"))
    };

    message
}

/// Describes one conversation for the switch notice: `the group `name`` or `a private chat`.
fn describe_conversation(is_group: bool, group_name: Option<&str>, group_id: &str) -> String {
    if !is_group {
        return "a private chat".to_string();
    }
    match group_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("the group `{name}`"),
        None => format!("the group `{group_id}`"),
    }
}

/// The notice that a steered message was sent in another conversation than the task's.
///
/// Steering is per sender, so a message sent in the sender's private chat can interrupt a
/// group turn (and the other way round). Without an explicit notice the model reads the
/// steered message as if it belonged to the running conversation.
fn conversation_switch_hint(
    turn_conversation: &str,
    turn_is_group: bool,
    turn_target_id: &str,
    steer_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
) -> Option<String> {
    let steer_is_group = steer_event.message_type
        == zihuan_core::ims_bot_adapter::models::event_model::MessageType::Group;
    if steer_is_group == turn_is_group {
        // Same kind: only a switch between different groups counts.
        if !steer_is_group {
            return None;
        }
        let steer_group_id = steer_event.group_id.map(|id| id.to_string()).unwrap_or_default();
        if steer_group_id == turn_target_id {
            return None;
        }
    }
    let steer_conversation = describe_conversation(
        steer_is_group,
        steer_event.group_name.as_deref(),
        &steer_event.group_id.map(|id| id.to_string()).unwrap_or_default(),
    );
    if steer_conversation == turn_conversation {
        return None;
    }
    Some(format!(
        "[Conversation Switch] The conversation has switched from {turn_conversation} to {steer_conversation}: \
         this steered message was sent in {steer_conversation}, while the current task and its reply belong to \
         {turn_conversation}. Keep the two conversations apart."
    ))
}

pub(crate) struct QqChatServiceSteerHook {
    pub(crate) pending_steer: Arc<PendingSteerStore>,
    pub(crate) sender_id: String,
    /// The conversation this turn runs in (group id, or the sender for private chat).
    /// Steers are aggregated per sender, so a message from another conversation of the
    /// same sender can interrupt this turn; these two fields detect that switch so the
    /// injected steer message can carry a conversation-switch notice.
    pub(crate) target_id: String,
    pub(crate) is_group: bool,
    /// The turn conversation's group name, for the switch notice's wording.
    pub(crate) turn_group_name: Option<String>,
    pub(crate) bot_id: String,
    pub(crate) bot_name: String,
    pub(crate) adapter: zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) max_steer_count: usize,
    pub(crate) llm_supports_multimodal_input: bool,
    pub(crate) s3_ref: Option<Arc<S3Ref>>,
    pub(crate) trace: QqChatTaskTrace,
    pub(crate) consumed_messages: Arc<Mutex<Vec<LLMMessage>>>,
    /// Steer events drained by this hook during the current turn but not yet covered
    /// by a sent reply. The turn's [`SteerCommitGuard`] commits or rolls them back.
    pub(crate) in_flight_steers: Arc<Mutex<Vec<PendingSteerEvent>>>,
    pub(crate) shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
    pub(crate) session_state: Arc<Mutex<QqChatSessionState>>,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    /// The turn context assembled by the before-brain procedure. The hook is built before the
    /// procedure chain runs, so the brain procedure writes the recalled memories into this
    /// shared cell before starting the inference loop.
    pub(crate) turn_context: Arc<Mutex<Option<SteerTurnContext>>>,
}

/// The assembled turn context a mid-turn steer message reuses.
///
/// A steer message is a new user turn inside the running loop, so it reuses the typed memory
/// recall of the turn it interrupts. The system prompt is not repeated: it already leads the
/// conversation as its system message.
#[derive(Clone)]
pub(crate) struct SteerTurnContext {
    pub(crate) memory_recall: Option<MemoryRecall>,
}

impl ToolCallingMiddleware for QqChatServiceSteerHook {
    fn on_before_inference(
        &self,
        _iteration: usize,
        _conversation: &[LLMMessage],
    ) -> Vec<LLMMessage> {
        let (pending, remaining_queue_len, accepted_steer_count) =
            self.pending_steer.drain_all(&self.sender_id);
        if pending.is_empty() {
            return Vec::new();
        }
        let steer_count = pending.len();
        self.in_flight_steers.lock().unwrap().extend(pending.iter().cloned());

        let mut injected = Vec::with_capacity(pending.len());
        let mut prepared_inputs = Vec::with_capacity(pending.len());
        let mut consumed_guard = self.consumed_messages.lock().unwrap();

        // The steer prefix leads the injected message, extended with a conversation-switch
        // notice for every steered conversation that differs from this turn's own: steering
        // is per sender, so a private message can interrupt a group turn and the other way
        // round, and without the notice the model reads the message as if it belonged to
        // the running conversation.
        let turn_conversation =
            describe_conversation(self.is_group, self.turn_group_name.as_deref(), &self.target_id);
        let mut prefix = STEER_PREFIX.to_string();
        for pending_event in &pending {
            if let Some(hint) = conversation_switch_hint(
                &turn_conversation,
                self.is_group,
                &self.target_id,
                &pending_event.event,
            ) {
                if !prefix.contains(&hint) {
                    prefix.push_str("\n\n");
                    prefix.push_str(&hint);
                }
            }
        }

        for pending_event in pending {
            let mut inference_event = pending_event.event.clone();
            inference_event.message_list =
                expand_messages_for_inference(&pending_event.event.message_list);
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

        let memory_recall = self
            .turn_context
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|context| context.memory_recall.clone());
        let steer_message = if prepared_inputs.len() == 1 {
            let mut session_state = self.session_state.lock().unwrap();
            build_steer_user_message(
                &prepared_inputs[0],
                &self.bot_name,
                &self.adapter,
                self.llm_supports_multimodal_input,
                &mut session_state,
                &self.emotion_dimensions,
                memory_recall.as_ref(),
            )
        } else {
            let mut session_state = self.session_state.lock().unwrap();
            build_merged_steer_user_message(
                &prepared_inputs,
                &self.bot_name,
                &self.adapter,
                self.llm_supports_multimodal_input,
                &mut session_state,
                &self.emotion_dimensions,
                memory_recall.as_ref(),
            )
        };
        let steer_message = apply_steer_prefix(steer_message, &prefix);
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

/// Tracks steer events the hook drained mid-turn that no reply covers yet.
///
/// The hook must drain before inference so the model sees the steers, but the turn
/// can still die afterwards — transport error, iteration limit, panic, any `?`
/// short-circuit. Dropping the guard without `commit` pushes those events back to
/// the front of the pending store, so the next turn answers them instead of losing
/// them silently.
pub(crate) struct SteerCommitGuard {
    pending_steer: Arc<PendingSteerStore>,
    sender_id: String,
    in_flight: Arc<Mutex<Vec<PendingSteerEvent>>>,
    committed: bool,
}

impl SteerCommitGuard {
    pub(crate) fn new(
        pending_steer: Arc<PendingSteerStore>,
        sender_id: &str,
        in_flight: Arc<Mutex<Vec<PendingSteerEvent>>>,
    ) -> Self {
        Self {
            pending_steer,
            sender_id: sender_id.to_string(),
            in_flight,
            committed: false,
        }
    }

    /// Mark the drained steers as covered by a reply (or a deliberate no-reply).
    pub(crate) fn commit(mut self) {
        self.committed = true;
        self.in_flight.lock().unwrap().clear();
    }
}

impl Drop for SteerCommitGuard {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let events = std::mem::take(&mut *self.in_flight.lock().unwrap());
        if events.is_empty() {
            return;
        }
        warn!(
            "{LOG_PREFIX} rolling back {} consumed steer(s) for {} after a turn that sent no reply",
            events.len(),
            self.sender_id
        );
        self.pending_steer.requeue_front(&self.sender_id, events);
    }
}

/// Handles an event that arrived while the sender's session is busy: executes it
/// immediately if it is a steer-bypass command, otherwise enqueues it as a steer event.
pub(crate) fn try_handle_busy_session_steer(
    service: &QqChatAgentService,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    ctx: &QqChatAgentServiceContext<'_>,
    sender_id: &str,
    target_id: &str,
    is_group: bool,
    time: &str,
) -> Result<()> {
    let bot_id = get_bot_id(ctx.adapter);
    let prepared_input =
        prepare_current_turn_user_input(event, ctx.adapter, &bot_id, ctx.bot_name, ctx.s3_ref);
    let mut inference_event = prepared_input.event.clone();
    inference_event.message_list =
        expand_messages_for_inference(&prepared_input.event.message_list);
    let current_message = prepare_current_turn_user_input_from_event(
        &inference_event,
        &bot_id,
        ctx.bot_name,
        ctx.s3_ref,
    )
    .current_text_for_prompt()
    .to_string();
    if let Some(command_registry) = zihuan_core::command::global_command_registry() {
        let cmd_ctx = CommandContext {
            agent_type: "qq_chat".to_string(),
            agent_id: service.id.clone(),
            caller_id: sender_id.to_string(),
            channel: CommandChannel::QqChat {
                sender_id: sender_id.to_string(),
                is_group,
                group_id: inference_event.group_id,
                target_id: target_id.to_string(),
            },
        };
        if let Some(preview) = command_registry.preview(&cmd_ctx, &current_message) {
            if preview.spec.allow_steer_bypass && preview.passthrough_text.is_none() {
                info!(
                    "{LOG_PREFIX} Session busy for {sender_id}, executing command via steer bypass: message_id={} command=/{}",
                    event.message_id,
                    preview.spec.name
                );
                let trace = QqChatTaskTrace::new(Local::now());
                let executed = crate::qq_chat::command::run_command_effects_now(
                    &trace,
                    ctx,
                    &service.id,
                    &prepared_input.event,
                    &inference_event,
                    sender_id,
                    target_id,
                    &bot_id,
                    is_group,
                    &current_message,
                )?;
                trace.finish_with_summary();
                if executed {
                    return Ok(());
                }
            } else {
                info!(
                    "{LOG_PREFIX} Session busy for {sender_id}, command falls back to steer: message_id={} command=/{} allow_steer_bypass={} has_passthrough={}",
                    event.message_id,
                    preview.spec.name,
                    preview.spec.allow_steer_bypass,
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
