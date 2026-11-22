use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;

use crate::data_refs::RelationalDbConnection;
use crate::error::Result;
use crate::model_inference::llm::LLMMessage;
use crate::qq_context::QqChatContextEvent;

macro_rules! database {
    ($connection:expr, |$pool:ident, $mysql:ident| $body:block) => {
        match $connection {
            RelationalDbConnection::MySql(config) => {
                let $pool = config
                    .pool
                    .as_ref()
                    .ok_or_else(|| crate::string_error!("context mysql pool unavailable"))?;
                let $mysql = true;
                $body
            }
            RelationalDbConnection::Sqlite(config) => {
                let $pool = config
                    .pool
                    .as_ref()
                    .ok_or_else(|| crate::string_error!("context sqlite pool unavailable"))?;
                let $mysql = false;
                $body
            }
        }
    };
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextWindow {
    pub messages: Vec<LLMMessage>,
    pub turn_ids: Vec<String>,
    #[serde(default)]
    pub message_turn_ids: Vec<Option<String>>,
    pub version: i64,
    pub estimated_tokens: i64,
    pub compacted: bool,
    pub association_incomplete: bool,
}

#[derive(Debug, Serialize)]
pub struct ContextSession {
    pub sender_id: String,
    pub sender_name: String,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub is_group: bool,
    pub last_time: Option<String>,
    pub context_tokens: i64,
    pub has_context: bool,
    pub window_version: i64,
}

#[derive(Debug, Serialize)]
pub struct ContextStep {
    pub step_id: String,
    pub turn_id: String,
    pub parent_step_id: Option<String>,
    pub sequence: i64,
    pub kind: String,
    pub name: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub revision: i64,
    pub model: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cached_prompt_tokens: Option<i64>,
    pub prompt_cache_miss_tokens: Option<i64>,
    pub first_token_ms: Option<f64>,
    pub duration_ms: Option<f64>,
    pub output_tokens_per_second: Option<f64>,
    pub error_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn stamp(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| {
            value
                .with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|_| value.to_string())
}

fn insert(mysql: bool) -> &'static str {
    if mysql {
        "INSERT IGNORE"
    } else {
        "INSERT OR IGNORE"
    }
}

fn stable_id(turn_id: &str, key: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, format!("{turn_id}:{key}").as_bytes())
        .to_string()
}

pub fn event_identity(event: &QqChatContextEvent) -> (&str, &str, &str) {
    match event {
        QqChatContextEvent::TurnStarted { agent_id, sender_id, turn_id, .. }
        | QqChatContextEvent::TurnFinished { agent_id, sender_id, turn_id, .. }
        | QqChatContextEvent::ToolStarted { agent_id, sender_id, turn_id, .. }
        | QqChatContextEvent::ToolFinished { agent_id, sender_id, turn_id, .. }
        | QqChatContextEvent::Stage { agent_id, sender_id, turn_id, .. }
        | QqChatContextEvent::SubAgent { agent_id, sender_id, turn_id, .. } => {
            (agent_id, sender_id, turn_id)
        }
    }
}

pub async fn record_event(
    connection: &RelationalDbConnection,
    event: &QqChatContextEvent,
) -> Result<()> {
    let (agent_id, sender_id, turn_id) = event_identity(event);
    let updated_at = now();
    match event {
        QqChatContextEvent::TurnStarted {
            sender_name,
            group_id,
            group_name,
            is_group,
            created_at,
            user_text,
            quoted,
            media,
            continues_previous,
            ..
        } => {
            let created = stamp(created_at);
            let sql_time = chrono::DateTime::parse_from_rfc3339(&created)
                .map_err(|error| crate::string_error!("invalid turn timestamp: {error}"))?
                .naive_utc()
                .to_string();
            database!(connection, |pool, mysql| {
                let mut transaction = pool.begin().await?;
                sqlx::query(&format!("{} INTO qq_chat_context_turn (turn_id, agent_id, sender_id, sender_name, group_id, group_name, is_group, created_at, user_text, quoted_json, media_json, continues_previous, finished, status, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 'running', ?)", insert(mysql)))
                    .bind(turn_id).bind(agent_id).bind(sender_id).bind(sender_name).bind(group_id)
                    .bind(if *is_group { group_name.clone() } else { None }).bind(*is_group as i64)
                    .bind(&sql_time).bind(user_text).bind(serde_json::to_string(quoted)?)
                    .bind(serde_json::to_string(media)?).bind(*continues_previous as i64).bind(&updated_at)
                    .execute(&mut *transaction).await?;
                sqlx::query(&format!(
                    "{} INTO qq_chat_context_session (agent_id, sender_id) VALUES (?, ?)",
                    insert(mysql)
                ))
                .bind(agent_id)
                .bind(sender_id)
                .execute(&mut *transaction)
                .await?;
                sqlx::query("UPDATE qq_chat_context_session SET latest_turn_id = ?, last_interaction_at = ?, sender_name = ?, group_id = ?, group_name = ?, is_group = ? WHERE agent_id = ? AND sender_id = ? AND (last_interaction_at IS NULL OR last_interaction_at < ? OR (last_interaction_at = ? AND latest_turn_id <= ?))")
                    .bind(turn_id).bind(&created).bind(sender_name).bind(if *is_group { group_id.clone() } else { None })
                    .bind(if *is_group { group_name.clone() } else { None }).bind(*is_group as i64)
                    .bind(agent_id).bind(sender_id).bind(&created).bind(&created).bind(turn_id)
                    .execute(&mut *transaction).await?;
                transaction.commit().await?;
            });
        }
        QqChatContextEvent::TurnFinished {
            assistant_text,
            reasoning_text,
            usage,
            generation_ms,
            output_tokens_per_second,
            ..
        } => {
            database!(connection, |pool, _mysql| {
                let mut transaction = pool.begin().await?;
                sqlx::query("UPDATE qq_chat_context_turn SET assistant_text = ?, reasoning_text = ?, prompt_tokens = ?, cached_prompt_tokens = ?, prompt_cache_miss_tokens = ?, completion_tokens = ?, total_tokens = ?, generation_ms = ?, output_tokens_per_second = ?, finished = 1, status = CASE WHEN status = 'failed' THEN status ELSE 'completed' END, updated_at = ? WHERE turn_id = ? AND status IN ('running', 'failed')")
                    .bind(assistant_text).bind(reasoning_text)
                    .bind(usage.as_ref().and_then(|usage| usage.prompt_tokens).map(|value| value as i64))
                    .bind(usage.as_ref().and_then(|usage| usage.cached_prompt_tokens).map(|value| value as i64))
                    .bind(usage.as_ref().and_then(|usage| usage.prompt_cache_miss_tokens).map(|value| value as i64))
                    .bind(usage.as_ref().and_then(|usage| usage.completion_tokens).map(|value| value as i64))
                    .bind(usage.as_ref().and_then(|usage| usage.total_tokens).map(|value| value as i64))
                    .bind(generation_ms.map(|value| value as i64)).bind(output_tokens_per_second)
                    .bind(&updated_at).bind(turn_id).execute(&mut *transaction).await?;
                sqlx::query("UPDATE qq_chat_context_turn SET prompt_tokens = COALESCE((SELECT SUM(prompt_tokens) FROM qq_chat_context_step WHERE turn_id = ? AND kind = 'model'), prompt_tokens), completion_tokens = COALESCE((SELECT SUM(completion_tokens) FROM qq_chat_context_step WHERE turn_id = ? AND kind = 'model'), completion_tokens), total_tokens = COALESCE((SELECT SUM(total_tokens) FROM qq_chat_context_step WHERE turn_id = ? AND kind = 'model'), total_tokens), cached_prompt_tokens = COALESCE((SELECT SUM(cached_prompt_tokens) FROM qq_chat_context_step WHERE turn_id = ? AND kind = 'model'), cached_prompt_tokens), prompt_cache_miss_tokens = COALESCE((SELECT SUM(prompt_cache_miss_tokens) FROM qq_chat_context_step WHERE turn_id = ? AND kind = 'model'), prompt_cache_miss_tokens) WHERE turn_id = ?")
                    .bind(turn_id).bind(turn_id).bind(turn_id).bind(turn_id).bind(turn_id).bind(turn_id).execute(&mut *transaction).await?;
                transaction.commit().await?;
            });
        }
        _ => {
            let payload = serde_json::to_value(event)?;
            let kind = payload["kind"].as_str().unwrap_or("stage");
            if payload["event"]["kind"] == "assistant_chunk"
                || (payload["detail"].get("stream_content").is_some()
                    || payload["detail"].get("stream_reasoning").is_some())
                    && payload["detail"]["status"] == "running"
            {
                return Ok(());
            }
            let path = payload
                .get("path")
                .or_else(|| payload["detail"].get("path"))
                .cloned()
                .unwrap_or(json!([]));
            let nested = &payload["event"];
            let call_id = payload["call_id"].as_str().or_else(|| nested["call_id"].as_str());
            let key = if let Some(call_id) = call_id {
                format!("tool:{path}:{call_id}")
            } else if let Some(id) = payload["id"].as_str() {
                format!("stage:{id}")
            } else {
                format!("event:{}", serde_json::to_string(&payload)?)
            };
            let step_id = stable_id(turn_id, &key);
            let detail = &payload["detail"];
            let running = kind == "tool_started"
                || nested["kind"] == "tool_started"
                || detail["status"] == "running";
            let tool_error = payload["result"]
                .as_str()
                .or_else(|| nested["result"].as_str())
                .is_some_and(crate::agent::tools::tool_result_reports_failure);
            let failed = detail["status"] == "error" || payload["stage"] == "错误" || tool_error;
            let status = if failed {
                "failed"
            } else if running {
                "running"
            } else {
                "completed"
            };
            let step_kind = if payload["stage"] == crate::qq_context::QQ_CONTEXT_STAGE_LLM {
                "model"
            } else if call_id.is_some() {
                "tool"
            } else {
                kind
            };
            let name = payload["stage"]
                .as_str()
                .or_else(|| payload["tool_name"].as_str())
                .or_else(|| nested["name"].as_str())
                .unwrap_or("sub_agent");
            let mut input = payload.clone();
            if input["detail"].get("request").is_some() {
                if let Some(detail) = input["detail"].as_object_mut() {
                    detail.remove("messages");
                }
            }
            let mut output = payload.clone();
            if let Some(detail) = output.get_mut("detail").and_then(Value::as_object_mut) {
                detail.remove("messages");
                detail.remove("request");
                detail.remove("tools");
                detail.remove("stream_content");
                detail.remove("stream_reasoning");
            }
            database!(connection, |pool, _mysql| {
                let mut transaction = pool.begin().await?;
                sqlx::query("UPDATE qq_chat_context_turn SET next_step_sequence = next_step_sequence + 1 WHERE turn_id = ?")
                    .bind(turn_id).execute(&mut *transaction).await?;
                let existing = sqlx::query(
                    "SELECT revision, status FROM qq_chat_context_step WHERE step_id = ?",
                )
                .bind(&step_id)
                .fetch_optional(&mut *transaction)
                .await?;
                if let Some(existing) = existing {
                    if existing.get::<String, _>("status") != "running" || running {
                        transaction.commit().await?;
                        return Ok(());
                    }
                } else {
                    let sequence: i64 = sqlx::query_scalar(
                        "SELECT next_step_sequence FROM qq_chat_context_turn WHERE turn_id = ?",
                    )
                    .bind(turn_id)
                    .fetch_one(&mut *transaction)
                    .await?;
                    let parent = if let Some(call) =
                        path.as_array().and_then(|path| path.last()).and_then(Value::as_str)
                    {
                        let mut parent_path = path.as_array().cloned().unwrap_or_default();
                        parent_path.pop();
                        let id = stable_id(turn_id, &format!("tool:{}:{call}", json!(parent_path)));
                        sqlx::query_scalar::<_, String>("SELECT step_id FROM qq_chat_context_step WHERE step_id = ? AND turn_id = ?")
                            .bind(id).bind(turn_id).fetch_optional(&mut *transaction).await?
                    } else {
                        None
                    };
                    sqlx::query("INSERT INTO qq_chat_context_step (step_id, turn_id, parent_step_id, sequence, kind, name, status, started_at, input_json) VALUES (?, ?, ?, ?, ?, ?, 'running', ?, ?)")
                        .bind(&step_id).bind(turn_id).bind(parent).bind(sequence).bind(step_kind).bind(name).bind(&updated_at)
                        .bind(serde_json::to_string(&input)?).execute(&mut *transaction).await?;
                }
                sqlx::query("UPDATE qq_chat_context_step SET status = ?, finished_at = ?, revision = revision + 1, output_json = ?, error_text = ?, model = ?, prompt_tokens = ?, completion_tokens = ?, total_tokens = ?, cached_prompt_tokens = ?, prompt_cache_miss_tokens = ?, first_token_ms = ?, duration_ms = ?, output_tokens_per_second = ? WHERE step_id = ? AND status = 'running'")
                    .bind(status).bind(if running { None } else { Some(&updated_at) }).bind(if running { None } else { Some(serde_json::to_string(&output)?) })
                    .bind(detail["error"].as_str()).bind(detail["model"].as_str())
                    .bind(detail["usage"]["prompt_tokens"].as_i64()).bind(detail["usage"]["completion_tokens"].as_i64())
                    .bind(detail["usage"]["total_tokens"].as_i64()).bind(detail["usage"]["cached_prompt_tokens"].as_i64())
                    .bind(detail["usage"]["prompt_cache_miss_tokens"].as_i64()).bind(detail["first_token_ms"].as_f64())
                    .bind(detail["duration_ms"].as_f64().or_else(|| payload["duration_ms"].as_f64()).or_else(|| nested["duration_ms"].as_f64()))
                    .bind(detail["output_tokens_per_second"].as_f64()).bind(&step_id).execute(&mut *transaction).await?;
                if payload["stage"] == "错误" {
                    sqlx::query("UPDATE qq_chat_context_turn SET status = 'failed', updated_at = CASE WHEN updated_at IS NULL THEN NULL ELSE ? END WHERE turn_id = ?")
                        .bind(&updated_at).bind(turn_id).execute(&mut *transaction).await?;
                }
                transaction.commit().await?;
            });
        }
    }
    Ok(())
}

pub async fn window(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
) -> Result<Option<ContextWindow>> {
    database!(connection, |pool, _mysql| {
        let row = sqlx::query("SELECT window_json, window_version, context_tokens FROM qq_chat_context_session WHERE agent_id = ? AND sender_id = ?")
            .bind(agent).bind(sender).fetch_optional(pool).await?;
        if let Some(row) = row {
            if let Some(raw) = row.get::<Option<String>, _>("window_json") {
                let mut window: ContextWindow = serde_json::from_str(&raw)?;
                window.version = row.get("window_version");
                window.estimated_tokens = row.get("context_tokens");
                return Ok(Some(window));
            }
        }
    });
    Ok(None)
}

pub async fn save_window(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    window: &mut ContextWindow,
) -> Result<()> {
    window.estimated_tokens =
        crate::model_inference::inference_function::compact_message::estimate_messages_tokens(
            &window.messages,
        ) as i64;
    database!(connection, |pool, mysql| {
        let mut transaction = pool.begin().await?;
        sqlx::query(&format!(
            "{} INTO qq_chat_context_session (agent_id, sender_id) VALUES (?, ?)",
            insert(mysql)
        ))
        .bind(agent)
        .bind(sender)
        .execute(&mut *transaction)
        .await?;
        let changed = sqlx::query("UPDATE qq_chat_context_session SET window_version = window_version + 1, window_json = ?, context_tokens = ?, has_context = ? WHERE agent_id = ? AND sender_id = ? AND window_version = ?")
                    .bind(serde_json::to_string(window)?).bind(window.estimated_tokens).bind((!window.messages.is_empty()) as i64)
            .bind(agent).bind(sender).bind(window.version).execute(&mut *transaction).await?;
        if changed.rows_affected() != 1 {
            return Err(crate::string_error!(
                "context window changed concurrently; reload before writing"
            ));
        }
        window.version += 1;
        transaction.commit().await?;
    });
    Ok(())
}

pub async fn sessions(
    connection: &RelationalDbConnection,
    agent: &str,
    offset: i64,
    limit: i64,
    query: Option<&str>,
) -> Result<(Vec<ContextSession>, i64)> {
    let pattern = query.map(str::trim).filter(|query| !query.is_empty()).map(|query| {
        format!("%{}%", query.replace('|', "||").replace('%', "|%").replace('_', "|_"))
    });
    let filter = if pattern.is_some() {
        " AND (sender_id LIKE ? ESCAPE '|' OR sender_name LIKE ? ESCAPE '|' OR group_id LIKE ? ESCAPE '|' OR group_name LIKE ? ESCAPE '|')"
    } else {
        ""
    };
    let count_sql = format!("SELECT COUNT(*) FROM qq_chat_context_session WHERE agent_id = ? AND latest_turn_id IS NOT NULL{filter}");
    let page_sql = format!("SELECT sender_id, sender_name, group_id, group_name, is_group, last_interaction_at, context_tokens, has_context, window_version FROM qq_chat_context_session WHERE agent_id = ? AND latest_turn_id IS NOT NULL{filter} ORDER BY last_interaction_at DESC, sender_id DESC LIMIT ? OFFSET ?");
    database!(connection, |pool, _mysql| {
        let mut count = sqlx::query_scalar::<_, i64>(&count_sql).bind(agent);
        let mut page = sqlx::query(&page_sql).bind(agent);
        if let Some(pattern) = &pattern {
            for _ in 0..4 {
                count = count.bind(pattern);
                page = page.bind(pattern);
            }
        }
        let total = count.fetch_one(pool).await?;
        let rows = page.bind(limit).bind(offset).fetch_all(pool).await?;
        let items = rows
            .into_iter()
            .map(|row| ContextSession {
                sender_id: row.get("sender_id"),
                sender_name: row.get::<Option<String>, _>("sender_name").unwrap_or_default(),
                group_id: row.get("group_id"),
                group_name: row.get("group_name"),
                is_group: row.get::<i64, _>("is_group") != 0,
                last_time: row.get("last_interaction_at"),
                context_tokens: row.get("context_tokens"),
                has_context: row.get::<i64, _>("has_context") != 0,
                window_version: row.get("window_version"),
            })
            .collect();
        Ok((items, total))
    })
}

pub async fn steps(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    turn: &str,
    step: Option<&str>,
) -> Result<Vec<ContextStep>> {
    let columns = "s.step_id, s.turn_id, s.parent_step_id, s.sequence, s.kind, s.name, s.status, s.started_at, s.finished_at, s.revision, s.model, s.prompt_tokens, s.completion_tokens, s.total_tokens, s.cached_prompt_tokens, s.prompt_cache_miss_tokens, s.first_token_ms, s.duration_ms, s.output_tokens_per_second, s.error_text";
    let sql = format!("SELECT {columns}{} FROM qq_chat_context_step s JOIN qq_chat_context_turn t ON t.turn_id = s.turn_id WHERE t.agent_id = ? AND t.sender_id = ? AND t.turn_id = ?{} ORDER BY s.sequence", if step.is_some() { ", s.input_json, s.output_json" } else { "" }, if step.is_some() { " AND s.step_id = ?" } else { "" });
    database!(connection, |pool, _mysql| {
        let mut query = sqlx::query(&sql).bind(agent).bind(sender).bind(turn);
        if let Some(step) = step {
            query = query.bind(step);
        }
        let rows = query.fetch_all(pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(ContextStep {
                    step_id: row.try_get("step_id")?,
                    turn_id: row.try_get("turn_id")?,
                    parent_step_id: row.try_get("parent_step_id")?,
                    sequence: row.try_get("sequence")?,
                    kind: row.try_get("kind")?,
                    name: row.try_get("name")?,
                    status: row.try_get("status")?,
                    started_at: row.try_get("started_at")?,
                    finished_at: row.try_get("finished_at")?,
                    revision: row.try_get("revision")?,
                    model: row.try_get("model")?,
                    prompt_tokens: row.try_get("prompt_tokens")?,
                    completion_tokens: row.try_get("completion_tokens")?,
                    total_tokens: row.try_get("total_tokens")?,
                    cached_prompt_tokens: row.try_get("cached_prompt_tokens")?,
                    prompt_cache_miss_tokens: row.try_get("prompt_cache_miss_tokens")?,
                    first_token_ms: row.try_get("first_token_ms")?,
                    duration_ms: row.try_get("duration_ms")?,
                    output_tokens_per_second: row.try_get("output_tokens_per_second")?,
                    error_text: row.try_get("error_text")?,
                    input: row
                        .try_get::<Option<String>, _>("input_json")
                        .ok()
                        .flatten()
                        .map(|raw| serde_json::from_str(&raw))
                        .transpose()?,
                    output: row
                        .try_get::<Option<String>, _>("output_json")
                        .ok()
                        .flatten()
                        .map(|raw| serde_json::from_str(&raw))
                        .transpose()?,
                })
            })
            .collect()
    })
}

pub async fn recover(connection: &RelationalDbConnection, agent: &str) -> Result<()> {
    database!(connection, |pool, _mysql| {
        let mut transaction = pool.begin().await?;
        sqlx::query("UPDATE qq_chat_context_step SET status = 'interrupted', finished_at = ?, revision = revision + 1 WHERE status = 'running' AND turn_id IN (SELECT turn_id FROM qq_chat_context_turn WHERE agent_id = ?)")
            .bind(now()).bind(agent).execute(&mut *transaction).await?;
        sqlx::query("UPDATE qq_chat_context_turn SET status = 'interrupted', finished = 1, updated_at = ? WHERE agent_id = ? AND status = 'running'")
            .bind(now()).bind(agent).execute(&mut *transaction).await?;
        transaction.commit().await?;
    });
    Ok(())
}

pub async fn running(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
) -> Result<bool> {
    database!(connection, |pool, _mysql| {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ? AND status = 'running'")
            .bind(agent).bind(sender).fetch_one(pool).await?;
        Ok(count > 0)
    })
}

pub(crate) fn session_backfill_sql(mysql: bool, one_agent: bool) -> String {
    let timestamp = if mysql {
        "DATE_FORMAT(t.created_at, '%Y-%m-%dT%H:%i:%s.000Z')"
    } else {
        "strftime('%Y-%m-%dT%H:%M:%fZ', t.created_at)"
    };
    let filter = if one_agent { "t.agent_id = ? AND " } else { "" };
    format!("{} INTO qq_chat_context_session (agent_id, sender_id, latest_turn_id, last_interaction_at, sender_name, group_id, group_name, is_group) SELECT t.agent_id, t.sender_id, t.turn_id, {timestamp}, t.sender_name, t.group_id, CASE WHEN t.is_group = 1 THEN t.group_name ELSE NULL END, t.is_group FROM qq_chat_context_turn t WHERE {filter}NOT EXISTS (SELECT 1 FROM qq_chat_context_turn newer WHERE newer.agent_id = t.agent_id AND newer.sender_id = t.sender_id AND (newer.created_at > t.created_at OR (newer.created_at = t.created_at AND newer.turn_id > t.turn_id)))", insert(mysql))
}

pub async fn migrate_legacy(connection: &RelationalDbConnection, agent: &str) -> Result<()> {
    crate::database::migration_task::progress(format!("角色 {agent}：回填会话摘要"));
    database!(connection, |pool, mysql| {
        sqlx::query(&session_backfill_sql(mysql, true))
            .bind(agent)
            .execute(pool)
            .await?;
    });
    migrate_legacy_steps(connection, agent, None).await
}

/// Allows archived turns to be viewed before their role has been restarted after an upgrade.
pub async fn has_legacy_steps(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    turn: &str,
) -> Result<bool> {
    database!(connection, |pool, _mysql| {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ? AND turn_id = ? AND entries_json IS NOT NULL")
            .bind(agent).bind(sender).bind(turn).fetch_one(pool).await?;
        Ok(count > 0)
    })
}

pub async fn migrate_legacy_steps(
    connection: &RelationalDbConnection,
    agent: &str,
    scope: Option<(&str, &str)>,
) -> Result<()> {
    // Startup and a page opening an old turn may overlap. Only migrations share this lock;
    // live conversations keep writing independently. Recheck after acquiring it because
    // the batch payload may already have been migrated by the other caller.
    static MIGRATION_TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let mut cursor = String::new();
    let mut migrated = 0usize;
    let mut steps = 0usize;
    let started = std::time::Instant::now();
    loop {
        let records: Vec<Value> = database!(connection, |pool, _mysql| {
            let sql = format!("SELECT turn_id, sender_id, entries_json FROM qq_chat_context_turn WHERE agent_id = ? AND turn_id > ? AND entries_json IS NOT NULL{} ORDER BY turn_id LIMIT 100", if scope.is_some() { " AND sender_id = ? AND turn_id = ?" } else { "" });
            let mut query = sqlx::query(&sql).bind(agent).bind(&cursor);
            if let Some((sender, turn)) = scope {
                query = query.bind(sender).bind(turn);
            }
            let rows = query.fetch_all(pool).await?;
            rows.into_iter().map(|row| json!({
                "turn_id": row.get::<String, _>("turn_id"), "sender_id": row.get::<String, _>("sender_id"),
                "entries": row.get::<Option<String>, _>("entries_json"),
            })).collect()
        });
        if records.is_empty() {
            crate::database::migration_task::progress(format!(
                "角色 {agent}：旧轨迹迁移完成，{migrated} 轮、{steps} 个步骤，耗时 {:.1}s",
                started.elapsed().as_secs_f64()
            ));
            break;
        }
        for row in records {
            let turn = row["turn_id"].as_str().unwrap();
            let sender = row["sender_id"].as_str().unwrap();
            let _turn_guard = MIGRATION_TURN.lock().await;
            if !has_legacy_steps(connection, agent, sender, turn).await? {
                cursor = turn.to_string();
                continue;
            }
            if let Some(raw) = row["entries"].as_str() {
                let entries: Vec<Value> = serde_json::from_str(raw)?;
                for (index, mut entry) in entries.into_iter().enumerate() {
                    steps += 1;
                    if index % 25 == 0 {
                        crate::database::migration_task::progress(format!("角色 {agent}：正在迁移轮次 {turn}，第 {} 个步骤，累计完成 {migrated} 轮", index + 1));
                    }
                    if entry["kind"] == "stage" && entry["id"].is_null() {
                        entry["id"] = json!(format!("legacy-{index}"));
                    }
                    entry["agent_id"] = json!(agent);
                    entry["sender_id"] = json!(sender);
                    entry["turn_id"] = json!(turn);
                    if entry["kind"] == "tool_call" {
                        let call = entry["call_id"].as_str().unwrap_or_default().to_string();
                        let name = entry["name"].as_str().unwrap_or_default().to_string();
                        record_event(
                            connection,
                            &QqChatContextEvent::ToolStarted {
                                agent_id: agent.to_string(),
                                sender_id: sender.to_string(),
                                turn_id: turn.to_string(),
                                call_id: call.clone(),
                                tool_name: name.clone(),
                                arguments: entry["arguments"].clone(),
                            },
                        )
                        .await?;
                        if let Some(result) = entry["result"].as_str() {
                            record_event(
                                connection,
                                &QqChatContextEvent::ToolFinished {
                                    agent_id: agent.to_string(),
                                    sender_id: sender.to_string(),
                                    turn_id: turn.to_string(),
                                    call_id: call,
                                    tool_name: name,
                                    result: result.to_string(),
                                    duration_ms: entry["duration_ms"].as_u64().unwrap_or(0),
                                },
                            )
                            .await?;
                        }
                    } else {
                        if entry["kind"] == "sub_agent" {
                            entry["id"] = json!(format!("legacy-{index}"));
                        }
                        let event: QqChatContextEvent = serde_json::from_value(entry)?;
                        record_event(connection, &event).await?;
                    }
                }
                database!(connection, |pool, _mysql| {
                    sqlx::query(
                        "UPDATE qq_chat_context_turn SET entries_json = NULL WHERE turn_id = ?",
                    )
                    .bind(turn)
                    .execute(pool)
                    .await?;
                });
            }
            cursor = turn.to_string();
            migrated += 1;
            crate::database::migration_task::progress(format!("角色 {agent}：已迁移 {migrated} 轮、{steps} 个步骤，当前轮次 {turn}，耗时 {:.1}s", started.elapsed().as_secs_f64()));
        }
    }
    Ok(())
}

pub async fn fail_unfinished(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    turn: &str,
) -> Result<()> {
    database!(connection, |pool, _mysql| {
        let mut transaction = pool.begin().await?;
        sqlx::query("UPDATE qq_chat_context_turn SET status = 'failed', finished = 1, updated_at = ? WHERE agent_id = ? AND sender_id = ? AND turn_id = ? AND status = 'running'")
            .bind(now()).bind(agent).bind(sender).bind(turn).execute(&mut *transaction).await?;
        sqlx::query("UPDATE qq_chat_context_step SET status = 'interrupted', finished_at = ?, revision = revision + 1 WHERE turn_id = ? AND status = 'running' AND EXISTS (SELECT 1 FROM qq_chat_context_turn WHERE turn_id = ? AND agent_id = ? AND sender_id = ? AND finished = 1)")
            .bind(now()).bind(turn).bind(turn).bind(agent).bind(sender).execute(&mut *transaction).await?;
        transaction.commit().await?;
    });
    Ok(())
}

pub async fn bootstrap_window(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    mut messages: Vec<LLMMessage>,
) -> Result<()> {
    for message in &mut messages {
        message.context_message_id = Some(uuid::Uuid::new_v4().to_string());
    }
    let mut window = ContextWindow {
        messages,
        association_incomplete: true,
        ..Default::default()
    };
    save_window(connection, agent, sender, &mut window).await?;
    database!(connection, |pool, _mysql| {
        sqlx::query("UPDATE qq_chat_context_session SET latest_turn_id = COALESCE(latest_turn_id, 'legacy-window'), sender_name = COALESCE(sender_name, ?) WHERE agent_id = ? AND sender_id = ?")
            .bind(sender).bind(agent).bind(sender).execute(pool).await?;
    });
    Ok(())
}

/// Turn headers only: step bodies are fetched independently when expanded.
pub async fn turns(
    connection: &RelationalDbConnection,
    agent: &str,
    sender: &str,
    ids: Option<&[String]>,
    before: Option<(&str, &str)>,
    limit: i64,
) -> Result<Vec<Value>> {
    let mut sql = "SELECT turn_id, sender_name, group_id, group_name, is_group, CAST(created_at AS CHAR) AS created_at, updated_at, user_text, quoted_json, media_json, assistant_text, reasoning_text, prompt_tokens, completion_tokens, total_tokens, cached_prompt_tokens, prompt_cache_miss_tokens, generation_ms, output_tokens_per_second, continues_previous, finished, status FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ?".to_string();
    if let Some(ids) = ids {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        sql.push_str(&format!(" AND turn_id IN ({})", vec!["?"; ids.len()].join(",")));
    }
    if before.is_some() {
        sql.push_str(" AND (created_at < ? OR (created_at = ? AND turn_id < ?))");
    }
    sql.push_str(" ORDER BY created_at DESC, turn_id DESC LIMIT ?");
    database!(connection, |pool, _mysql| {
        let mut query = sqlx::query(&sql).bind(agent).bind(sender);
        if let Some(ids) = ids {
            for id in ids {
                query = query.bind(id);
            }
        }
        if let Some((time, id)) = before {
            query = query.bind(time).bind(time).bind(id);
        }
        let rows = query.bind(limit).fetch_all(pool).await?;
        rows.into_iter().map(|row| -> Result<Value> {
            let cursor_time: String = row.try_get("created_at")?;
            let created_at = if row.try_get::<Option<String>, _>("updated_at")?.is_some() {
                chrono::NaiveDateTime::parse_from_str(&cursor_time, "%Y-%m-%d %H:%M:%S%.f")
                    .map(|time| time.and_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                    .unwrap_or_else(|_| cursor_time.clone())
            } else { cursor_time.clone() };
            let parse = |name| -> Result<Value> {
                Ok(row.try_get::<Option<String>, _>(name)?.map(|raw| serde_json::from_str(&raw)).transpose()?.unwrap_or(Value::Null))
            };
            Ok(json!({
                "turn_id": row.try_get::<String, _>("turn_id")?, "sender_name": row.try_get::<Option<String>, _>("sender_name")?,
                "group_id": row.try_get::<Option<String>, _>("group_id")?, "group_name": row.try_get::<Option<String>, _>("group_name")?,
                "is_group": row.try_get::<i64, _>("is_group")? != 0, "created_at": created_at, "cursor_time": cursor_time,
                "user_text": row.try_get::<String, _>("user_text")?, "quoted": parse("quoted_json")?, "media": parse("media_json")?,
                "assistant_text": row.try_get::<Option<String>, _>("assistant_text")?, "reasoning_text": row.try_get::<Option<String>, _>("reasoning_text")?,
                "usage": { "prompt_tokens": row.try_get::<Option<i64>, _>("prompt_tokens")?, "completion_tokens": row.try_get::<Option<i64>, _>("completion_tokens")?,
                    "total_tokens": row.try_get::<Option<i64>, _>("total_tokens")?, "cached_prompt_tokens": row.try_get::<Option<i64>, _>("cached_prompt_tokens")?, "prompt_cache_miss_tokens": row.try_get::<Option<i64>, _>("prompt_cache_miss_tokens")? },
                "generation_ms": row.try_get::<Option<i64>, _>("generation_ms")?, "output_tokens_per_second": row.try_get::<Option<f64>, _>("output_tokens_per_second")?,
                "continues_previous": row.try_get::<i64, _>("continues_previous")? != 0, "finished": row.try_get::<i64, _>("finished")? != 0,
                "status": row.try_get::<String, _>("status")?, "entries": [], "steps_available": true,
            }))
        }).collect()
    })
}
