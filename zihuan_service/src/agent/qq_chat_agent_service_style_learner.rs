use std::collections::HashMap;
use std::sync::Arc;

use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{InferenceParam, LLMMessage};
use zihuan_core::task_context::{scope_task_id, scope_task_runtime, AgentTaskResult, AgentTaskStatus};

use crate::agent::qq_chat_agent_service_language_style_store::{upsert_language_style, LanguageStyleScope};
use crate::agent::qq_chat_agent_service_logging::QqChatTaskTrace;
use crate::agent::qq_chat_agent_service_msg_send::{
    build_reply_result, send_planned_batches, QqChatServiceSendContext,
};
use crate::agent::tools::{review_and_rewrite_reply, QqReplyReviewRequest};

const STYLE_LEARNING_SAMPLE_LIMIT: i64 = 200;
const STYLE_LEARNING_MIN_SAMPLES: usize = 20;

struct StyleSample {
    text: String,
    char_count: usize,
}

pub struct StyleLearningOutcome {
    pub updated: bool,
    pub sample_count: usize,
    pub style_prompt: Option<String>,
    pub summary: String,
}

pub async fn learn_language_style(
    connection: &RelationalDbConnection,
    scope: &LanguageStyleScope,
    llm: &Arc<dyn LLMBase>,
    learned_by_sender_id: &str,
) -> Result<StyleLearningOutcome> {
    let samples = fetch_style_learning_samples(connection, scope).await?;
    if samples.len() < STYLE_LEARNING_MIN_SAMPLES {
        return Ok(StyleLearningOutcome {
            updated: false,
            sample_count: samples.len(),
            style_prompt: None,
            summary: "样本不足，未更新风格。".to_string(),
        });
    }

    let messages = build_style_learning_messages(scope, &samples);
    let llm_clone = Arc::clone(llm);
    let response = tokio::task::spawn_blocking(move || {
        llm_clone.inference(&InferenceParam {
            messages: &messages,
            tools: None,
        })
    })
    .await
    .map_err(|e| Error::StringError(format!("style learning LLM task panicked: {e}")))?;
    let style_prompt = parse_style_learning_result(&response.content_text_owned().unwrap_or_default())?;
    let saved = upsert_language_style(
        connection,
        scope,
        &style_prompt,
        samples.len() as i32,
        learned_by_sender_id,
    )
    .await?;

    Ok(StyleLearningOutcome {
        updated: true,
        sample_count: saved.sample_count as usize,
        style_prompt: Some(saved.style_prompt),
        summary: format!("语言风格学习完成，已更新风格。样本数：{}", saved.sample_count),
    })
}

fn build_style_learning_messages(scope: &LanguageStyleScope, samples: &[StyleSample]) -> Vec<LLMMessage> {
    let scope_label = match scope {
        LanguageStyleScope::Global => "全局聊天",
        LanguageStyleScope::Group { .. } => "当前群聊",
    };
    let system = "你是语言风格学习器。你的任务是根据聊天样本，提炼出一段可以直接插入到 user message 前缀里的语言风格提示词。\
除了说话风格、口吻、常用表达、称呼偏好、禁忌之外，还必须分析并总结消息文本长短特征，包括：\
1) 每条消息的典型字数范围（偏短句还是长段落）；\
2) 是否习惯将内容拆成多条短消息连续发送，还是倾向于合并为一条长消息；\
3) 句子节奏（是否喜欢用短句断句、是否爱用省略号或换行分段）。\
输出必须是严格 JSON：{\"style_prompt\": string, \"reason\": string}。\
style_prompt 只允许描述说话风格、口吻、常用表达、称呼偏好、句长节奏、消息长短习惯、禁忌，不得逐句复述原文，不得泄露敏感信息。\
每条样本前标注了字数，请据此分析消息长短分布。";
    let user = format!(
        "请根据以下{scope_label}样本学习语言风格（含消息长短习惯），并输出单段风格提示词。\n\n样本条数: {}\n\n{}",
        samples.len(),
        samples
            .iter()
            .enumerate()
            .map(|(index, sample)| format!("{}. [{}字] {}", index + 1, sample.char_count, sample.text))
            .collect::<Vec<_>>()
            .join("\n")
    );
    vec![LLMMessage::system(system.to_string()), LLMMessage::user(user)]
}

fn parse_style_learning_result(content: &str) -> Result<String> {
    let value: serde_json::Value = serde_json::from_str(content.trim())
        .map_err(|err| Error::ValidationError(format!("style learning returned invalid json: {err}")))?;
    let style_prompt = value
        .get("style_prompt")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("style learning result missing style_prompt".to_string()))?;
    Ok(style_prompt.to_string())
}

async fn fetch_style_learning_samples(
    connection: &RelationalDbConnection,
    scope: &LanguageStyleScope,
) -> Result<Vec<StyleSample>> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| Error::ValidationError("style-learning mysql pool is not initialized".to_string()))?;
            let rows = match scope {
                LanguageStyleScope::Global => {
                    sqlx::query(
                        "SELECT sender_name, content FROM message_record \
                         WHERE content IS NOT NULL AND TRIM(content) <> '' ORDER BY send_time DESC LIMIT ?",
                    )
                    .bind(STYLE_LEARNING_SAMPLE_LIMIT)
                    .fetch_all(pool)
                    .await
                    .map_err(Error::Database)?
                }
                LanguageStyleScope::Group { group_id } => {
                    sqlx::query(
                        "SELECT sender_name, content FROM message_record \
                         WHERE group_id = ? AND content IS NOT NULL AND TRIM(content) <> '' \
                         ORDER BY send_time DESC LIMIT ?",
                    )
                    .bind(group_id)
                    .bind(STYLE_LEARNING_SAMPLE_LIMIT)
                    .fetch_all(pool)
                    .await
                    .map_err(Error::Database)?
                }
            };
            Ok(normalize_mysql_rows(rows))
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| Error::ValidationError("style-learning sqlite pool is not initialized".to_string()))?;
            let rows = match scope {
                LanguageStyleScope::Global => {
                    sqlx::query(
                        "SELECT sender_name, content FROM message_record \
                         WHERE content IS NOT NULL AND TRIM(content) <> '' ORDER BY send_time DESC LIMIT ?",
                    )
                    .bind(STYLE_LEARNING_SAMPLE_LIMIT)
                    .fetch_all(pool)
                    .await
                    .map_err(Error::Database)?
                }
                LanguageStyleScope::Group { group_id } => {
                    sqlx::query(
                        "SELECT sender_name, content FROM message_record \
                         WHERE group_id = ? AND content IS NOT NULL AND TRIM(content) <> '' \
                         ORDER BY send_time DESC LIMIT ?",
                    )
                    .bind(group_id)
                    .bind(STYLE_LEARNING_SAMPLE_LIMIT)
                    .fetch_all(pool)
                    .await
                    .map_err(Error::Database)?
                }
            };
            Ok(normalize_sqlite_rows(rows))
        }
    }
}

fn normalize_mysql_rows(rows: Vec<MySqlRow>) -> Vec<StyleSample> {
    rows.into_iter()
        .filter_map(|row| {
            let content: String = row.get("content");
            let content = content.trim().to_string();
            if content.is_empty() {
                return None;
            }
            let sender_name: Option<String> = row.try_get("sender_name").ok();
            let text = match sender_name.map(|item| item.trim().to_string()).filter(|item| !item.is_empty()) {
                Some(sender_name) => format!("{sender_name}: {content}"),
                None => content,
            };
            Some(StyleSample {
                char_count: text.chars().count(),
                text,
            })
        })
        .collect()
}

fn normalize_sqlite_rows(rows: Vec<SqliteRow>) -> Vec<StyleSample> {
    rows.into_iter()
        .filter_map(|row| {
            let content: String = row.get("content");
            let content = content.trim().to_string();
            if content.is_empty() {
                return None;
            }
            let sender_name: Option<String> = row.try_get("sender_name").ok();
            let text = match sender_name.map(|item| item.trim().to_string()).filter(|item| !item.is_empty()) {
                Some(sender_name) => format!("{sender_name}: {content}"),
                None => content,
            };
            Some(StyleSample {
                char_count: text.chars().count(),
                text,
            })
        })
        .collect()
}

fn run_blocking_future<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new()?.block_on(future)
    }
}

#[derive(Clone)]
pub(crate) struct OwnedStyleLearningTaskContext {
    pub adapter: ims_bot_adapter::adapter::SharedBotAdapter,
    pub bot_name: String,
    pub natural_language_reply_llm: Arc<dyn zihuan_core::llm::llm_base::LLMBase>,
    pub intent_classification_llm: Arc<dyn zihuan_core::llm::llm_base::LLMBase>,
    pub rdb_pool: RelationalDbConnection,
    pub max_message_length: usize,
    pub reply_batch_builder: Option<crate::agent::qq_chat_agent_service_core::QqChatServiceReplyBatchBuilder>,
    pub resolved_language_style_prompt: Option<String>,
}

#[derive(Clone)]
pub(crate) struct StyleLearningResumeInput {
    pub event: ims_bot_adapter::models::MessageEvent,
    pub inference_event: ims_bot_adapter::models::MessageEvent,
    pub sender_id: String,
    pub target_id: String,
    pub bot_id: String,
    pub is_group: bool,
    pub scope: LanguageStyleScope,
}

pub(crate) fn execute_style_learning_task(
    owned: OwnedStyleLearningTaskContext,
    input: StyleLearningResumeInput,
    trace: QqChatTaskTrace,
    task_handle: Arc<zihuan_core::task_context::AgentTaskHandle>,
    task_runtime: Arc<dyn zihuan_core::task_context::AgentTaskRuntime>,
) {
    let task_handle_for_panic = Arc::clone(&task_handle);
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<String> {
        scope_task_runtime(task_runtime, || {
            scope_task_id(task_handle.task_id.clone(), || {
                let _ = zihuan_core::task_context::append_current_task_progress(
                    "开始执行语言风格学习".to_string(),
                );
                let learning = run_blocking_future(learn_language_style(
                    &owned.rdb_pool,
                    &input.scope,
                    &owned.natural_language_reply_llm,
                    &input.sender_id,
                ))?;
                if !learning.updated {
                    return Ok(learning.summary);
                }
                let _ = zihuan_core::task_context::append_current_task_progress(
                    format!("已学习 {} 条样本", learning.sample_count),
                );
                let feedback_base = format!(
                    "语言风格学习完成，已更新{}风格提示词。样本数：{}。",
                    if matches!(input.scope, LanguageStyleScope::Global) {
                        "全局"
                    } else {
                        "当前群聊"
                    },
                    learning.sample_count
                );
                let review_result = review_and_rewrite_reply(
                    &owned.intent_classification_llm,
                    &owned.natural_language_reply_llm,
                    Some("请确保反馈消息也符合刚刚学到的语言风格。"),
                    &QqReplyReviewRequest {
                        candidate_message: feedback_base.clone(),
                        is_group: input.is_group,
                        bot_name: owned.bot_name.clone(),
                        sender_id: input.sender_id.clone(),
                        sender_nickname: input.inference_event.sender.nickname.clone(),
                        sender_card: input.inference_event.sender.card.clone(),
                        session_state: zihuan_agent::session_state::QqChatAgentServiceSessionState::default(),
                        emotion_dimensions: Vec::new(),
                        available_media_ids: Vec::new(),
                    },
                    &trace,
                )?;
                let final_feedback = if owned
                    .resolved_language_style_prompt
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_some()
                {
                    review_result.final_message
                } else {
                    format!(
                        "{}\n{}",
                        review_result.final_message,
                        learning.style_prompt.clone().unwrap_or_default()
                    )
                };
                let send_ctx = QqChatServiceSendContext {
                    adapter: &owned.adapter,
                    target_id: &input.target_id,
                    is_group: input.is_group,
                    group_name: input.event.group_name.as_deref(),
                    bot_id: &input.bot_id,
                    bot_name: &owned.bot_name,
                    mention_target_id: None,
                    persistence: crate::storage::qq_chat_session_store::build_outbound_persistence(
                        Some(&owned.rdb_pool),
                        input.event.group_name.as_deref(),
                        &owned.bot_name,
                    ),
                    max_text_chars: owned.max_message_length,
                };
                let reply_result = build_reply_result(
                    &final_feedback,
                    input.is_group,
                    &input.sender_id,
                    &input.inference_event.sender.nickname,
                    &input.inference_event.sender.card,
                    &input.bot_id,
                    &owned.bot_name,
                    owned.max_message_length,
                    None,
                    None,
                    HashMap::new(),
                    owned.reply_batch_builder.as_ref(),
                )?;
                if !reply_result.suppress_send && !reply_result.batches.is_empty() {
                    send_planned_batches(&send_ctx, &reply_result.batches);
                }
                Ok(learning.summary)
            })
        })
    }));

    match run {
        Ok(Ok(summary)) => task_handle_for_panic.finish(AgentTaskResult {
            status: Some(AgentTaskStatus::Success),
            result_summary: Some(summary),
            error_message: None,
        }),
        Ok(Err(err)) => task_handle_for_panic.finish(AgentTaskResult {
            status: Some(AgentTaskStatus::Failed),
            result_summary: Some(format!("语言风格学习失败: {err}")),
            error_message: Some(err.to_string()),
        }),
        Err(_panic) => task_handle_for_panic.finish(AgentTaskResult {
            status: Some(AgentTaskStatus::Failed),
            result_summary: Some("语言风格学习任务意外终止".to_string()),
            error_message: Some("task panicked".to_string()),
        }),
    }
}
