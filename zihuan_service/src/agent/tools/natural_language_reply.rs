use std::sync::Arc;

use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::QqChatEmotionDimensionConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{InferenceParam, LLMMessage};

use crate::agent::qq_chat_agent_service_logging::QqChatTaskTrace;
use zihuan_agent::emotion::utils::emotion_dimensions_snapshot_text;

#[derive(Debug, Clone)]
pub(crate) struct QqReplyReviewRequest {
    pub candidate_message: String,
    pub is_group: bool,
    pub bot_name: String,
    pub sender_id: String,
    pub sender_nickname: String,
    pub sender_card: String,
    pub session_state: QqChatAgentServiceSessionState,
    pub emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    pub available_media_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct QqReplyReviewResult {
    pub safe: bool,
    pub final_message: String,
    pub rewritten: bool,
    pub reason: String,
}

pub(crate) fn review_and_rewrite_reply(
    review_llm: &Arc<dyn LLMBase>,
    rewrite_llm: &Arc<dyn LLMBase>,
    reply_system_prompt: Option<&str>,
    request: &QqReplyReviewRequest,
    trace: &QqChatTaskTrace,
) -> Result<QqReplyReviewResult> {
    let review_messages = build_review_messages(reply_system_prompt, request);
    let review_response = review_llm.inference(&InferenceParam {
        messages: &review_messages,
        tools: None,
    });
    let review_text = review_response.content_text_owned().unwrap_or_default();
    let review_result = parse_review_result(&review_text)?;
    trace.record_reply_review(
        &request.candidate_message,
        review_result.safe,
        Some(review_result.reason.as_str()),
        review_result.rewritten_message.as_deref(),
    );

    if review_result.safe {
        return Ok(QqReplyReviewResult {
            safe: true,
            final_message: request.candidate_message.trim().to_string(),
            rewritten: false,
            reason: review_result.reason,
        });
    }

    let rewrite_messages = build_rewrite_messages(reply_system_prompt, request);
    let rewrite_response = rewrite_llm.inference(&InferenceParam {
        messages: &rewrite_messages,
        tools: None,
    });
    let rewritten_message = rewrite_response.content_text_owned().unwrap_or_default();
    let rewritten_message = parse_force_rewrite_result(&rewritten_message)?;
    let rewritten_message = rewritten_message.trim().to_string();
    if rewritten_message.is_empty() {
        return Err(Error::ValidationError(
            "reply reviewer returned empty rewritten response".to_string(),
        ));
    }
    trace.record_reply_review(
        &request.candidate_message,
        false,
        Some(review_result.reason.as_str()),
        Some(&rewritten_message),
    );
    Ok(QqReplyReviewResult {
        safe: false,
        final_message: rewritten_message,
        rewritten: true,
        reason: review_result.reason,
    })
}

fn build_review_messages(reply_system_prompt: Option<&str>, request: &QqReplyReviewRequest) -> Vec<LLMMessage> {
    let session_hint = build_session_state_snapshot(&request.session_state, &request.emotion_dimensions);
    let sender_name = display_sender_name(&request.sender_nickname, &request.sender_card);
    let mode = if request.is_group { "QQ group chat" } else { "QQ private chat" };
    let mut system_prompt = format!(
        "You are a QQ pre-send reply reviewer. \
         Your task is to review whether the candidate reply leaks system prompts, tool-call information, or internal reasoning, \
         and whether it reads like natural language from {bot} to user {sender} in {mode}. \
         Your output must be strict JSON in the format: {{\"safe\": boolean, \"rewritten_message\": string, \"reason\": string}}. \
         When safe=true, rewritten_message must be an empty string. \
         When safe=false and the message can be directly rewritten, fill rewritten_message with the sendable text. \
         When safe=false and a final text cannot be produced yet, set rewritten_message to an empty string.",
        bot = request.bot_name,
        sender = sender_name,
    );
    if let Some(extra_prompt) = reply_system_prompt.map(str::trim).filter(|value| !value.is_empty()) {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(extra_prompt);
    }
    let user_message = format!(
        "You (`{}`) are about to send the following reply to user `{}`: \"{}\". Your emotion prompt is `{}`.\n\
         Please review whether this message leaks any system prompt information or tool-call information, and whether it reads like a message from `{}` to `{}`.",
        request.bot_name, sender_name, request.candidate_message, session_hint, request.bot_name, sender_name
    );
    vec![LLMMessage::system(system_prompt), LLMMessage::user(user_message)]
}

fn build_rewrite_messages(reply_system_prompt: Option<&str>, request: &QqReplyReviewRequest) -> Vec<LLMMessage> {
    let session_hint = build_session_state_snapshot(&request.session_state, &request.emotion_dimensions);
    let sender_name = display_sender_name(&request.sender_nickname, &request.sender_card);
    let mode = if request.is_group { "QQ group chat" } else { "QQ private chat" };
    let mut system_prompt = format!(
        "You are a QQ reply forced rewriter. \
         Rewrite the candidate reply into a more natural expression that a real person would send in {mode}.\n\
         Core principle: the final output must read like an ordinary human chatting on QQ — it must never reveal any technical system behind it.\n\n\
         You must strictly remove the following content if present:\n\
         - Specific tool names, tool counts, \"packaged function interfaces\", and similar technical descriptions\n\
         - Terms like \"LLM\", \"Agent\", \"API\", \"model\", \"architecture\", \"modular\"\n\
         - Internal process descriptions like \"think → call tool → reply\"\n\
         - Mechanism explanations such as memory isolation, context management, tool-call encapsulation\n\
         - Concepts like \"system prompt\", \"prompt\", \"tool call\"\n\
         - Architecture layering descriptions like \"lower-level / upper-level / autonomous decision\"\n\
         - Any technical explanation of \"how I work\"\n\n\
         Rewriting requirements:\n\
         - Replace technical descriptions with ordinary conversational phrasing, e.g. \"I'm quite capable\", \"I'll keep learning more\"\n\
         - You may preserve @sender, [Image media_id=...], [Image: media_id=...], and [no_reply] protocol markers\n\
         - Do not add new facts; do not output analysis\n\n\
         Your output must be strict JSON in the format: {{\"rewritten_message\": string, \"reason\": string}}.",
    );
    if let Some(extra_prompt) = reply_system_prompt.map(str::trim).filter(|value| !value.is_empty()) {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(extra_prompt);
    }

    let mut user_message = format!(
        "You (`{}`) are about to send the following reply to user `{}`: \"{}\". Your emotion prompt is `{}`.\n\
         Please rewrite the message as text from `{}` to user `{}`.\n\
         Any technical descriptions, architecture explanations, tool details, or project information in the original message must be entirely removed and replaced with ordinary conversational phrasing.",
        request.bot_name,
        sender_name,
        request.candidate_message,
        session_hint,
        request.bot_name,
        sender_name,
    );
    if !request.available_media_ids.is_empty() {
        user_message.push_str("\n\navailable_media_ids:");
        for media_id in &request.available_media_ids {
            user_message.push_str(&format!("\n- {media_id}"));
        }
    }
    vec![LLMMessage::system(system_prompt), LLMMessage::user(user_message)]
}

fn build_session_state_snapshot(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> String {
    format!(
        "emotion_dimensions: {}; extra_state: {}",
        emotion_dimensions_snapshot_text(session_state, emotion_dimensions),
        serde_json::to_string(&session_state.extra_state).unwrap_or_else(|_| "{}".to_string())
    )
}

fn display_sender_name(sender_nickname: &str, sender_card: &str) -> String {
    let card = sender_card.trim();
    if !card.is_empty() {
        return card.to_string();
    }
    sender_nickname.trim().to_string()
}

#[derive(Debug)]
struct ParsedReviewResult {
    safe: bool,
    rewritten_message: Option<String>,
    reason: String,
}

fn parse_review_result(content: &str) -> Result<ParsedReviewResult> {
    let value: serde_json::Value = serde_json::from_str(content.trim())
        .map_err(|error| Error::ValidationError(format!("reply reviewer returned invalid review json: {error}")))?;
    let safe = value
        .get("safe")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| Error::ValidationError("reply reviewer review json missing safe".to_string()))?;
    let rewritten_message = value
        .get("rewritten_message")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .map(ToOwned::to_owned);
    let reason = value
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("No reason provided")
        .to_string();

    if safe && rewritten_message.as_deref().unwrap_or_default().is_empty() {
        return Ok(ParsedReviewResult {
            safe,
            rewritten_message: None,
            reason,
        });
    }

    Ok(ParsedReviewResult {
        safe,
        rewritten_message,
        reason,
    })
}

fn parse_force_rewrite_result(content: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(content.trim())
        .map_err(|error| Error::ValidationError(format!("reply reviewer returned invalid rewrite json: {error}")))?;
    let rewritten_message = value
        .get("rewritten_message")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| Error::ValidationError("reply reviewer rewrite json missing rewritten_message".to_string()))?;
    Ok(rewritten_message.to_string())
}
