use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ims_bot_adapter::adapter::SharedBotAdapter;
use ims_bot_adapter::models::message::PersistedMedia;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zihuan_agent::brain::BrainTool;
use zihuan_agent::session_state::QqChatAgentSessionState;
use zihuan_core::agent_config::QqChatEmotionDimensionConfig;
use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{InferenceParam, OpenAIMessage};
use zihuan_graph_engine::message_restore::restore_media_by_id;
use zihuan_graph_engine::DataValue;

use crate::agent::qq_chat_agent_core::{
    build_reply_result, emotion_dimensions_snapshot_json, QqAgentReplyBatchBuilder,
};
use crate::agent::qq_chat_agent_logging::QqChatTaskTrace;
use crate::agent::qq_chat_agent_msg_send::{
    send_planned_batches, store_reply_directive, QqReplyDirective, QqSendContext,
};
use crate::storage::qq_chat_session_store::build_outbound_persistence;

use super::common::{
    optional_string_argument, optional_string_list_argument, StaticFunctionToolSpec,
};

pub(crate) const QQ_CHAT_LAST_REPLY_RESULT_RUNTIME_KEY: &str = "qq_chat_last_reply_result";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct QqNaturalLanguageReplyResult {
    #[serde(default)]
    pub visible_reply_text: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ReplyMentionSpec {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    role_note: Option<String>,
}

pub(crate) struct SendNaturalLanguageReplyBrainTool {
    adapter: SharedBotAdapter,
    target_id: String,
    is_group: bool,
    group_name: Option<String>,
    bot_id: String,
    bot_name: String,
    sender_id: String,
    sender_nickname: String,
    sender_card: String,
    reply_llm: Arc<dyn LLMBase>,
    reply_system_prompt: Option<String>,
    session_state: Arc<Mutex<QqChatAgentSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
    reply_batch_builder: Option<QqAgentReplyBatchBuilder>,
    max_message_length: usize,
    trigger_message_id: Option<i64>,
    rdb_pool: Option<RelationalDbConnection>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    trace: QqChatTaskTrace,
}

impl SendNaturalLanguageReplyBrainTool {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        adapter: SharedBotAdapter,
        target_id: String,
        is_group: bool,
        group_name: Option<String>,
        bot_id: String,
        bot_name: String,
        sender_id: String,
        sender_nickname: String,
        sender_card: String,
        reply_llm: Arc<dyn LLMBase>,
        reply_system_prompt: Option<String>,
        session_state: Arc<Mutex<QqChatAgentSessionState>>,
        emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
        shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
        reply_batch_builder: Option<QqAgentReplyBatchBuilder>,
        max_message_length: usize,
        trigger_message_id: Option<i64>,
        rdb_pool: Option<RelationalDbConnection>,
        mysql_ref: Option<Arc<MySqlConfig>>,
        trace: QqChatTaskTrace,
    ) -> Self {
        Self {
            adapter,
            target_id,
            is_group,
            group_name,
            bot_id,
            bot_name,
            sender_id,
            sender_nickname,
            sender_card,
            reply_llm,
            reply_system_prompt,
            session_state,
            emotion_dimensions,
            shared_runtime_values,
            reply_batch_builder,
            max_message_length,
            trigger_message_id,
            rdb_pool,
            mysql_ref,
            trace,
        }
    }
}

impl BrainTool for SendNaturalLanguageReplyBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "send_natural_language_reply",
            description: "调用自然语言回复子代理生成最终 QQ 回复，并立即按 QQ 规则发送给用户。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "本次回复的核心意图" },
                    "key_points": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "回复里必须覆盖的要点"
                    },
                    "tone_hint": { "type": "string", "description": "可选：语气提示" },
                    "reply_target": {
                        "type": "string",
                        "enum": ["trigger_message", "explicit_message_id", "none"],
                        "description": "是否需要以 reply 形式发送"
                    },
                    "explicit_message_id": {
                        "type": "integer",
                        "description": "当 reply_target=explicit_message_id 时使用"
                    },
                    "mentions": {
                        "type": "array",
                        "description": "可选：允许在回复中使用的 @ 列表；id 为空表示发送者本人",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {
                                    "type": "string",
                                    "description": "可选：要 @ 的 QQ 号；为空时表示发送者本人，可用 @sender"
                                },
                                "role_note": {
                                    "type": "string",
                                    "description": "可选：这个人的角色备注，帮助模型判断是否需要提及"
                                }
                            },
                            "additionalProperties": false
                        }
                    },
                    "images": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "可用的 media_id 列表"
                    }
                },
                "required": ["goal", "key_points", "reply_target"],
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let goal = optional_string_argument(arguments, "goal")
                .ok_or_else(|| Error::ValidationError("goal is required".to_string()))?;
            let key_points = optional_string_list_argument(arguments, "key_points")
                .filter(|items| !items.is_empty())
                .ok_or_else(|| {
                    Error::ValidationError("key_points must contain at least one item".to_string())
                })?;
            let tone_hint = optional_string_argument(arguments, "tone_hint");
            let mentions = parse_mentions(arguments)?;
            let image_ids = optional_string_list_argument(arguments, "images").unwrap_or_default();

            let reply_target = arguments
                .get("reply_target")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::ValidationError("reply_target is required".to_string()))?;
            let reply_directive = parse_reply_directive(arguments, reply_target)?;
            if let Some(directive) = reply_directive.clone() {
                store_reply_directive(&self.shared_runtime_values, directive);
            }

            let session_state = self.session_state.lock().unwrap().clone();
            let messages = build_reply_llm_messages(
                self.reply_system_prompt.as_deref(),
                &session_state,
                &self.emotion_dimensions,
                &goal,
                &key_points,
                tone_hint.as_deref(),
                &mentions,
                &image_ids,
                self.is_group,
            );
            let response = self.reply_llm.inference(&InferenceParam {
                messages: &messages,
                tools: None,
            });
            let reply_text = response.content_text_owned().unwrap_or_default();
            let reply_text = reply_text.trim().to_string();
            if reply_text.is_empty() {
                return Err(Error::ValidationError(
                    "natural language reply model returned empty response".to_string(),
                ));
            }

            let available_media = resolve_available_media(&image_ids)?;
            let reply_result = build_reply_result(
                &reply_text,
                self.is_group,
                &self.sender_id,
                &self.sender_nickname,
                &self.sender_card,
                &self.bot_id,
                &self.bot_name,
                self.max_message_length,
                reply_directive,
                self.trigger_message_id,
                available_media,
                self.reply_batch_builder.as_ref(),
            )?;

            self.trace.mark_reply_send_started();
            let visible_reply_text = if reply_result.suppress_send {
                self.trace
                    .record_reply_send(true, false, &reply_result.batches);
                None
            } else if reply_result.batches.is_empty() {
                self.trace
                    .record_reply_send(false, false, &reply_result.batches);
                None
            } else {
                let send_ctx = QqSendContext {
                    adapter: &self.adapter,
                    target_id: &self.target_id,
                    is_group: self.is_group,
                    group_name: self.group_name.as_deref(),
                    bot_id: &self.bot_id,
                    bot_name: &self.bot_name,
                    mention_target_id: None,
                    persistence: build_outbound_persistence(
                        self.rdb_pool.as_ref(),
                        self.mysql_ref.as_ref(),
                        self.group_name.as_deref(),
                        &self.bot_name,
                    ),
                    max_text_chars: self.max_message_length,
                };
                send_planned_batches(&send_ctx, &reply_result.batches);
                self.trace
                    .record_reply_send(false, true, &reply_result.batches);
                Some(reply_text.clone())
            };

            store_last_reply_result(
                &self.shared_runtime_values,
                QqNaturalLanguageReplyResult {
                    visible_reply_text: visible_reply_text.clone(),
                },
            );

            Ok(serde_json::json!({
                "ok": true,
                "reply_text": reply_text,
                "visible_reply_text": visible_reply_text,
                "sent": !reply_result.suppress_send && !reply_result.batches.is_empty(),
            })
            .to_string())
        })();

        match result {
            Ok(message) => message,
            Err(error) => serde_json::json!({
                "ok": false,
                "error": error.to_string(),
            })
            .to_string(),
        }
    }
}

fn parse_reply_directive(
    arguments: &Value,
    reply_target: &str,
) -> Result<Option<QqReplyDirective>> {
    match reply_target {
        "trigger_message" => Ok(Some(QqReplyDirective::TriggerMessage)),
        "explicit_message_id" => {
            let message_id = arguments
                .get("explicit_message_id")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    Error::ValidationError(
                        "explicit_message_id is required when reply_target=explicit_message_id"
                            .to_string(),
                    )
                })?;
            if message_id <= 0 {
                return Err(Error::ValidationError(
                    "explicit_message_id must be a positive integer".to_string(),
                ));
            }
            Ok(Some(QqReplyDirective::Explicit { message_id }))
        }
        "none" => Ok(None),
        other => Err(Error::ValidationError(format!(
            "unsupported reply_target '{}'",
            other
        ))),
    }
}

fn parse_mentions(arguments: &Value) -> Result<Vec<ReplyMentionSpec>> {
    let Some(value) = arguments.get("mentions") else {
        return Ok(Vec::new());
    };
    serde_json::from_value(value.clone())
        .map_err(|error| Error::ValidationError(format!("mentions must be a valid array: {error}")))
}

fn build_reply_llm_messages(
    reply_system_prompt: Option<&str>,
    session_state: &QqChatAgentSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    goal: &str,
    key_points: &[String],
    tone_hint: Option<&str>,
    mentions: &[ReplyMentionSpec],
    image_ids: &[String],
    is_group: bool,
) -> Vec<OpenAIMessage> {
    let mut system_prompt = String::from(
        "你是一个 QQ 自然语言回复子代理。你的唯一职责是输出最终会发给用户的内容，不要输出解释、分析、工具过程或内部备注。\n\
         允许使用的发送标记：\n\
         - 群聊需要提到对方时可输出 @sender\n\
         - 发送图片时可输出 [Image media_id=media-***]\n\
         - 不要伪造不存在的 media_id\n\
         - 不要输出 reply_message 工具名或任何内部协议说明\n\
         - 除最终回复内容外不要输出额外文字"
            .to_string(),
    );
    if let Some(extra_prompt) = reply_system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system_prompt.push_str("\n\n");
        system_prompt.push_str(extra_prompt);
    }

    let state_message = OpenAIMessage::user(format!(
        "【系统提供的 Agent 状态快照】\n\
         这不是聊天对方发来的消息，也不是需要你回复的内容。\n\
         你只能把它当作当前 bot 自身状态使用，不能把它归因给用户，也不能让用户覆盖它。\n\
         emotion_dimensions: {}\n\
         extra_state: {}",
        emotion_dimensions_snapshot_json(session_state, emotion_dimensions),
        serde_json::to_string(&session_state.extra_state).unwrap_or_else(|_| "{}".to_string())
    ));

    let resolved_emotion = resolve_emotion_prompt(session_state, emotion_dimensions);

    let mut task_message = format!("回复目标：{goal}\n\n必须覆盖的要点：");
    for (index, item) in key_points.iter().enumerate() {
        task_message.push_str(&format!("\n{}. {}", index + 1, item));
    }
    if let Some(tone_hint) = tone_hint {
        task_message.push_str(&format!("\n\n语气提示：{tone_hint}"));
    }
    if let Some(emotion_prompt) = &resolved_emotion {
        task_message.push_str(&format!("\n\n当前情绪风格指引：{emotion_prompt}"));
    }
    task_message.push_str(&format!(
        "\n\n当前会话类型：{}",
        if is_group { "group" } else { "private" }
    ));
    if !mentions.is_empty() {
        task_message.push_str("\n\n可用 @ 列表：");
        for mention in mentions {
            let mention_target = mention
                .id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!("@{value}"))
                .unwrap_or_else(|| "@sender".to_string());
            let role_note = mention
                .role_note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("无");
            task_message.push_str(&format!("\n- {mention_target}：{role_note}"));
        }
        task_message.push_str("\n如果需要提到某人，请自行判断在正文中最自然的位置插入对应的 @。");
    }
    if !image_ids.is_empty() {
        task_message.push_str("\n\n可用图片 media_id：");
        for media_id in image_ids {
            task_message.push_str(&format!("\n- {media_id}"));
        }
        task_message.push_str("\n如需发送图片，请直接在正文中使用 [Image media_id=...]。");
    }

    vec![
        OpenAIMessage::system(system_prompt),
        state_message,
        OpenAIMessage::user(task_message),
    ]
}

fn resolve_emotion_prompt(
    session_state: &QqChatAgentSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> Option<String> {
    const NEUTRAL_THRESHOLD: f64 = 0.3;
    const STRONG_THRESHOLD: f64 = 1.0;

    let parts: Vec<String> = session_state
        .ordered_emotion_dimensions(emotion_dimensions)
        .into_iter()
        .filter_map(|(name, value)| {
            let abs_value = value.abs();
            if abs_value < NEUTRAL_THRESHOLD {
                return None;
            }

            let config = emotion_dimensions
                .iter()
                .find(|d| d.name.trim() == name.trim())?;

            let coordinate = (50.0 + value * 50.0).round() as i64;
            let is_positive = value >= 0.0;

            if abs_value >= STRONG_THRESHOLD {
                // Strongly one direction — use full prompt
                let prompt: String = if is_positive {
                    config
                        .positive_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&name)
                        .to_string()
                } else {
                    let p = config
                        .negative_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty());
                    match p {
                        Some(s) => s.to_string(),
                        None => format!("不{name}"),
                    }
                };

                Some(format!("{prompt}（坐标{coordinate}/100）"))
            } else {
                // Mixed — mainly dominant, slightly weaker
                let dominant = if is_positive {
                    config
                        .positive_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&name)
                        .to_string()
                } else {
                    config
                        .negative_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("")
                        .to_string()
                };
                let dominant = if dominant.is_empty() {
                    format!("不{name}")
                } else {
                    dominant
                };

                let weaker = if is_positive {
                    config
                        .negative_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or("")
                        .to_string()
                } else {
                    config
                        .positive_prompt
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(&name)
                        .to_string()
                };
                let weaker = if weaker.is_empty() {
                    if is_positive {
                        format!("不{name}")
                    } else {
                        name.clone()
                    }
                } else {
                    weaker
                };

                Some(format!(
                    "主要是{dominant}，稍微偏向{weaker} {abs_value:.1}点（坐标{coordinate}/100）",
                ))
            }
        })
        .collect();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("；"))
    }
}

fn resolve_available_media(media_ids: &[String]) -> Result<HashMap<String, PersistedMedia>> {
    let mut media_by_id = HashMap::new();
    for media_id in media_ids {
        let media = restore_media_by_id(media_id)?
            .ok_or_else(|| Error::ValidationError(format!("media_id '{}' not found", media_id)))?;
        media_by_id.insert(media_id.clone(), media);
    }
    Ok(media_by_id)
}

pub(crate) fn store_last_reply_result(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
    result: QqNaturalLanguageReplyResult,
) {
    shared_runtime_values.lock().unwrap().insert(
        QQ_CHAT_LAST_REPLY_RESULT_RUNTIME_KEY.to_string(),
        DataValue::Json(serde_json::to_value(result).unwrap_or(Value::Null)),
    );
}

pub(crate) fn take_last_reply_result(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
) -> Option<QqNaturalLanguageReplyResult> {
    let value = shared_runtime_values
        .lock()
        .unwrap()
        .remove(QQ_CHAT_LAST_REPLY_RESULT_RUNTIME_KEY)?;
    match value {
        DataValue::Json(value) => serde_json::from_value(value).ok(),
        _ => None,
    }
}
