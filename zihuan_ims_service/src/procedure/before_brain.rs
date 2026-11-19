//! Deterministic input assembly for a QQ turn.
//!
//! Before the main brain runs, the turn's input is assembled without any model inference:
//! the system prompt is built from the common rules plus the role service's own prompt, the
//! current turn's user input embeds the retrieved memory titles and the emotion prompt, and
//! the finished input is handed to the brain procedure through the shared procedure context.
//! The brain therefore never assembles the turn itself; it only appends history and runs the
//! tool-calling loop.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use serde_json::json;

use zihuan_core::agent::tools::memory::MemoryRecall;
use zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::qq_context::emit_qq_context_stage;
use zihuan_core::steer::message_with_api_style;
use zihuan_core::steer::PROCESSING_INSTRUCTION;

use crate::agent::emotion::utils::{emotion_expression_prompt, has_noticeable_emotion_expression};
use crate::qq_chat::logging::{QqChatTaskTrace, QqChatTurnIdentity};
use crate::qq_chat::user_input::PreparedCurrentTurnUserInput;
use crate::qq_chat::{build_group_system_prompt, build_private_system_prompt, build_user_message};
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;

/// Everything a turn's input assembly needs, borrowed from the turn context.
///
/// **Design:** Constructed per turn by the transport adapter and consumed by the before-brain
/// procedure. It is pure data — no model, no tool loop, no persisted transcript — because the
/// procedure only formats the turn's context.
pub(crate) struct BeforeBrainContext<'a> {
    pub(crate) planer: Option<&'a crate::qq_chat::planer::ImsPlanerInput>,
    pub(crate) trace: &'a QqChatTaskTrace,
    /// Live context-view identity. When set, the assembly stages land in the Data Explorer
    /// turn flow; the trace always receives the same phases.
    pub(crate) identity: Option<QqChatTurnIdentity>,
    /// The routed turn model. Assembly reads its API style and multimodal capability so the
    /// produced user message matches the model the brain will use.
    pub(crate) llm: &'a Arc<dyn LLMBase>,
    pub(crate) input: &'a PreparedCurrentTurnUserInput,
    pub(crate) adapter: &'a SharedBotAdapter,
    pub(crate) bot_name: &'a str,
    pub(crate) is_group: bool,
    /// The role service's own system prompt, appended to the common system rules.
    pub(crate) agent_system_prompt: Option<&'a str>,
    pub(crate) style_prompt: Option<&'a str>,
    pub(crate) message_rate_limit_warning: Option<&'a str>,
    pub(crate) session_state: Arc<Mutex<QqChatSessionState>>,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
}

/// The assembled turn context handed from the before-brain procedure to the brain procedure.
#[derive(Clone)]
pub(crate) struct BeforeBrainOutput {
    /// System prompt assembled from the common rules and the role service's prompt.
    pub(crate) system_prompt: String,
    /// The current turn's user input, including the memory-title hint and emotion prompt.
    pub(crate) user_message: LLMMessage,
    /// The typed memory recall injected into the user input. Mid-turn steer messages render the
    /// same recall so they stay consistent with the turn they interrupt.
    pub(crate) memory_recall: Option<MemoryRecall>,
}

/// Assemble this turn's system prompt and user input. No inference runs here.

pub(crate) fn assemble_before_brain(
    ctx: &BeforeBrainContext<'_>,
    reply_instruction: Option<&str>,
    memory_recall: Option<MemoryRecall>,
) -> BeforeBrainOutput {
    let started_at = Instant::now();

    // The language style is suppressed while the turn already expresses a noticeable emotion;
    // the decision is made before the prompt so it lands in the system message.
    let style_prompt_suppressed = {
        let session_state = ctx.session_state.lock().unwrap();
        has_noticeable_emotion_expression(&session_state, &ctx.emotion_dimensions)
    };
    let effective_style_prompt = if style_prompt_suppressed {
        None
    } else {
        ctx.style_prompt
    };

    // System prompt: the shared QQ chat rules, the role service's prompt, the reply output
    //    contract and the turn's language style. It leads the conversation as its system
    //    message, so the user turns stay free of it.
    let mut system_prompt = if ctx.is_group {
        build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt, effective_style_prompt)
    } else {
        build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt, effective_style_prompt)
    };
    if let Some(planer) = ctx.planer {
        system_prompt.push_str(&zihuan_core::role::planer::prompt_for_context(&planer.context));
        system_prompt.push_str(&format!(
            "\n你是本次 ims_send 的执行者 {}（Role ID：{}），已接到发送任务。来源 Planer {} 是委托方，不是发送通道；收件目标为 {} {}。\n你需要根据指令生成给该收件人的消息正文。最终文本会在本轮推理结束后，由当前 IMS Role 的审查、格式化和发送链路自动投递，并由运行时把真实发送回执返回委托方。执行本次发送不需要也不得再次调用 planer_request；只有确需辅助信息时才可向 Planer 请求协助。\n最终输出只能是给收件人的正文，不要向委托方汇报，不要输出发送成功/失败报告或内部调用链、工具错误解释。指令中要求的发送结果由运行时回报，你不能在实际投递前声称成功或失败。工具失败不等于当前 IMS 发送失败；仍可生成正文时应继续完成。应尽力生成可发送正文；确实无法生成安全正文时输出 [no_reply]，运行时会将任务标记失败。没有真实入站消息，不要使用 @sender 或引用不存在的触发消息。",
            ctx.bot_name, planer.context.role_id, planer.source_planer_id,
            planer.target_type, planer.target_id,
        ));
    } else if let Some(identity) = &ctx.identity {
        system_prompt.push_str(&zihuan_core::role::planer::prompt(&identity.agent_id));
    }
    ctx.trace
        .record_graph_stage("system prompt", json!({ "prompt": system_prompt }));
    if let Some(identity) = ctx.identity.as_ref() {
        emit_qq_context_stage(
            &identity.agent_id,
            &identity.sender_id,
            &identity.turn_id,
            zihuan_core::qq_context::QQ_CONTEXT_STAGE_SYSTEM_PROMPT,
            json!({ "prompt": system_prompt }),
        );
    }

    let (user_message, emotion_prompt, emotion_values) = {
        let mut session_state = ctx.session_state.lock().unwrap();
        let emotion_prompt = emotion_expression_prompt(&session_state, &ctx.emotion_dimensions);
        let emotion_values = session_state
            .ordered_emotion_dimensions(&ctx.emotion_dimensions)
            .into_iter()
            .map(|(name, value)| (name, json!(value)))
            .collect::<serde_json::Map<String, serde_json::Value>>();
        let user_message = if let Some(planer) = ctx.planer {
            LLMMessage::user(format!(
                "委托方 Planer ID: {}；QQ 收件目标 {} {}\n发送指令：{}\n情绪指导：{}\n召回记忆：{}\n请生成本次要投递给上述收件人的正文；发送及向委托方回报回执由运行时完成。",
                planer.source_planer_id,
                planer.target_type,
                planer.target_id,
                planer.prompt,
                emotion_prompt,
                memory_recall
                    .as_ref()
                    .map(crate::agent::utils::memory_recall_trace)
                    .map(|value| value.to_string())
                    .unwrap_or_default()
            ))
        } else {
            message_with_api_style(
                build_user_message(
                    ctx.input,
                    ctx.bot_name,
                    ctx.adapter,
                    ctx.llm.supports_multimodal_input(),
                    ctx.message_rate_limit_warning,
                    &mut session_state,
                    &ctx.emotion_dimensions,
                    memory_recall.as_ref(),
                    reply_instruction,
                    crate::qq_chat::DEFAULT_REPLY_REQUEST,
                    PROCESSING_INSTRUCTION,
                ),
                ctx.llm.api_style(),
            )
        };
        (user_message, emotion_prompt, emotion_values)
    };

    // Report the emotion prompt the input actually carries; a noticeable emotion expression
    // replaces the configured language style.
    ctx.trace.record_graph_stage(
        "情绪提示词",
        json!({
            "emotion_prompt": emotion_prompt,
            "style_prompt_suppressed": style_prompt_suppressed,
            "style_prompt": ctx.style_prompt,
        }),
    );
    if let Some(identity) = ctx.identity.as_ref() {
        emit_qq_context_stage(
            &identity.agent_id,
            &identity.sender_id,
            &identity.turn_id,
            "情绪与风格提示",
            json!({
                "emotion_dimensions": emotion_values,
                "emotion_prompt": emotion_prompt,
                "style_prompt_suppressed": style_prompt_suppressed,
                "style_prompt": ctx.style_prompt,
            }),
        );
    }

    let user_input_text = user_message.content_text_owned().unwrap_or_default();
    let memory_trace = memory_recall.as_ref().map(crate::agent::utils::memory_recall_trace);
    ctx.trace.record_graph_stage(
        "User Input 组装",
        json!({
            "user_input": user_input_text,
            "memory_recall": memory_trace,
            "duration_ms": started_at.elapsed().as_millis() as u64,
        }),
    );
    if let Some(identity) = ctx.identity.as_ref() {
        emit_qq_context_stage(
            &identity.agent_id,
            &identity.sender_id,
            &identity.turn_id,
            "User Input",
            json!({
                "user_input": user_input_text,
                "memory_recall": memory_trace,
            }),
        );
    }

    BeforeBrainOutput {
        system_prompt,
        user_message,
        memory_recall,
    }
}
