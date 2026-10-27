//! Deterministic input assembly for a QQ turn.
//!
//! Before the main brain runs, the turn's input is assembled without any model inference:
//! the system prompt is built from the common rules plus the role service's own prompt, the
//! current turn's user input embeds the retrieved memory titles and the emotion prompt, and
//! the finished input is handed to the brain procedure through the shared procedure context.
//! The brain therefore never assembles the turn itself; it only appends history and runs the
//! tool-calling loop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::warn;
use serde_json::json;

use zihuan_core::agent::tools::memory_tools::{relevant_memory_titles, MemoryTitleHit};
use zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::qq_context::emit_qq_context_stage;
use zihuan_core::steer::message_with_api_style;

use crate::agent::emotion::utils::{emotion_expression_prompt, has_noticeable_emotion_expression};
use crate::qq_chat::logging::{QqChatTaskTrace, QqChatTurnIdentity};
use crate::qq_chat::user_input::PreparedCurrentTurnUserInput;
use crate::qq_chat::{build_group_system_prompt, build_private_system_prompt, build_user_message};
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;
use crate::tools::{AgentMemoryToolResources, DEFAULT_TOOL_MEMORY_AGENT};

const LOG_PREFIX: &str = "[QqBeforeBrain]";

/// How many memory titles the retrieval store may return for the turn's hint.
const MEMORY_HINT_TOP_N: usize = 10;

/// Retrieval relevance a memory title must reach to be listed. Backends normalize relevance as
/// "higher is better"; backends that do not rank (local-file memory) report `None` and are
/// listed whenever the query matches.
const MEMORY_HINT_MIN_RELEVANCE: f64 = 0.5;

/// Everything a turn's input assembly needs, borrowed from the turn context.
///
/// **Design:** Constructed per turn by the transport adapter and consumed by the before-brain
/// procedure. It is pure data — no model, no tool loop, no persisted transcript — because the
/// procedure only formats the turn's context.
pub(crate) struct BeforeBrainContext<'a> {
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
    pub(crate) memory_resources: Option<AgentMemoryToolResources>,
    pub(crate) default_tools_enabled: &'a HashMap<String, bool>,
}

/// The assembled turn context handed from the before-brain procedure to the brain procedure.
#[derive(Clone)]
pub(crate) struct BeforeBrainOutput {
    /// System prompt assembled from the common rules and the role service's prompt.
    pub(crate) system_prompt: String,
    /// The current turn's user input, including the memory-title hint and emotion prompt.
    pub(crate) user_message: LLMMessage,
    /// The memory-title hint block injected into the user input. Mid-turn steer messages embed
    /// the same block so they stay consistent with the turn they interrupt.
    pub(crate) memory_context: Option<String>,
}

/// A memory-title hint prepared for the turn.
struct MemoryHint {
    /// Block injected into the state/system prefix of the user input.
    context: String,
    query: String,
    candidates: usize,
    titles: Vec<MemoryTitleHit>,
}

impl MemoryHint {
    /// Context-view payload describing what the retrieval store returned for this turn.
    fn stage_detail(&self) -> serde_json::Value {
        json!({
            "query": self.query,
            "top_n": MEMORY_HINT_TOP_N,
            "min_relevance": MEMORY_HINT_MIN_RELEVANCE,
            "candidates": self.candidates,
            "titles": self
                .titles
                .iter()
                .map(|hit| json!({ "title": hit.title, "relevance": hit.relevance }))
                .collect::<Vec<_>>(),
        })
    }
}

/// Assemble this turn's system prompt and user input. No inference runs here.
pub(crate) fn assemble_before_brain(ctx: &BeforeBrainContext<'_>) -> BeforeBrainOutput {
    let started_at = Instant::now();

    // 1. System prompt: the shared QQ chat rules plus the role service's system prompt.
    let system_prompt = if ctx.is_group {
        build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
    } else {
        build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
    };
    ctx.trace
        .record_graph_phase("system prompt", json!({ "prompt": system_prompt }));
    if let Some(identity) = ctx.identity.as_ref() {
        emit_qq_context_stage(
            &identity.agent_id,
            &identity.sender_id,
            &identity.turn_id,
            zihuan_core::qq_context::QQ_CONTEXT_STAGE_SYSTEM_PROMPT,
            json!({ "prompt": system_prompt }),
        );
    }

    // 2. Memory titles: list what the retrieval store already relates to this message so the
    //    brain knows which memories exist and can search the missing ones with `memory_agent`.
    let memory_hint = build_memory_hint(ctx);
    if let Some(hint) = memory_hint.as_ref() {
        let detail = hint.stage_detail();
        ctx.trace.record_graph_phase("记忆检索", detail.clone());
        if let Some(identity) = ctx.identity.as_ref() {
            emit_qq_context_stage(
                &identity.agent_id,
                &identity.sender_id,
                &identity.turn_id,
                "记忆检索",
                detail,
            );
        }
    }

    // 3. User input: emotion prompt + state prefix + memory hint + the current event, in the
    //    shape the routed model accepts.
    let memory_context = memory_hint.as_ref().map(|hint| hint.context.clone());
    let (user_message, emotion_prompt, style_prompt_suppressed, emotion_values) = {
        let mut session_state = ctx.session_state.lock().unwrap();
        let emotion_prompt = emotion_expression_prompt(&session_state, &ctx.emotion_dimensions);
        let emotion_values = session_state
            .ordered_emotion_dimensions(&ctx.emotion_dimensions)
            .into_iter()
            .map(|(name, value)| (name, json!(value)))
            .collect::<serde_json::Map<String, serde_json::Value>>();
        let style_prompt_suppressed =
            has_noticeable_emotion_expression(&session_state, &ctx.emotion_dimensions);
        let user_message = message_with_api_style(
            build_user_message(
                ctx.input,
                ctx.bot_name,
                ctx.adapter,
                ctx.llm.supports_multimodal_input(),
                &system_prompt,
                ctx.style_prompt,
                ctx.message_rate_limit_warning,
                &mut session_state,
                &ctx.emotion_dimensions,
                memory_context.as_deref(),
            ),
            ctx.llm.api_style(),
        );
        (user_message, emotion_prompt, style_prompt_suppressed, emotion_values)
    };

    // Report the emotion prompt the input actually carries; a noticeable emotion expression
    // replaces the configured language style.
    ctx.trace.record_graph_phase(
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
    ctx.trace.record_graph_phase(
        "User Input 组装",
        json!({
            "user_input": user_input_text,
            "memory_context": memory_context,
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
                "has_memory_hint": memory_context.is_some(),
            }),
        );
    }

    BeforeBrainOutput {
        system_prompt,
        user_message,
        memory_context,
    }
}

/// Retrieves the memory titles related to the current message and renders the hint block.
///
/// Returns `None` when the turn has no memory backend, when the memory agent tool is disabled,
/// or when the retrieval fails; the turn then proceeds without a memory hint instead of
/// failing.
fn build_memory_hint(ctx: &BeforeBrainContext<'_>) -> Option<MemoryHint> {
    let resources = ctx.memory_resources.as_ref()?;
    if !ctx
        .default_tools_enabled
        .get(DEFAULT_TOOL_MEMORY_AGENT)
        .copied()
        .unwrap_or(true)
    {
        return None;
    }
    let query = ctx.input.current_text_for_prompt().to_string();
    let hits = match relevant_memory_titles(resources, &query, MEMORY_HINT_TOP_N) {
        Ok(hits) => hits,
        Err(error) => {
            warn!("{LOG_PREFIX} memory title retrieval failed: {error}");
            return None;
        }
    };
    let candidates = hits.len();
    let titles = hits
        .into_iter()
        .filter(|hit| {
            hit.relevance.map_or(true, |relevance| relevance >= MEMORY_HINT_MIN_RELEVANCE)
        })
        .collect::<Vec<_>>();

    let context = if titles.is_empty() {
        "No memory title in the retrieval store looks related to the current message. \
         If the current message depends on past conversations, saved facts, or user preferences, \
         call `memory_agent` to search the missing memory before answering."
            .to_string()
    } else {
        let mut lines = vec![
            "These memory titles from the retrieval store are the most relevant to the current \
             message:"
                .to_string(),
        ];
        for hit in &titles {
            match hit.relevance {
                Some(relevance) => {
                    lines.push(format!("- {} (relevance {relevance:.2})", hit.title));
                }
                None => lines.push(format!("- {}", hit.title)),
            }
        }
        lines.push(
            "When you need the details of a listed title, or when the memory you need is not \
             listed here, call `memory_agent` to search it instead of guessing."
                .to_string(),
        );
        lines.join("\n")
    };

    Some(MemoryHint { context, query, candidates, titles })
}
