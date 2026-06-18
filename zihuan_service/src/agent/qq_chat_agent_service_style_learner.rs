use std::sync::Arc;

use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{InferenceParam, LLMMessage};

use crate::agent::qq_chat_agent_service_language_style_store::{upsert_language_style, LanguageStyleScope};

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
