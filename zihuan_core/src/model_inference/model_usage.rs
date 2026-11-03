use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use tokio::sync::mpsc;

use super::llm::llm_base::{LLMBase, StreamingLLMBase};
use super::llm::{InferenceParam, LLMMessage, StreamToken, TokenUsage};
use crate::error::Result;
use crate::model_inference::inference_function::compact_message::estimate_messages_tokens;
use crate::model_inference::message_content_utils::raw_message_content;
use crate::qq_context::{with_turn_trace_context, QQ_CONTEXT_STAGE_LLM};

/// Minimum gap between two live stream updates of one inference bubble. Each update carries
/// the full accumulated text, so emitting per token would scale quadratically with the reply.
const LLM_STREAM_UPDATE_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelUsageRecord {
    pub config_id: String,
    pub used_at: DateTime<Utc>,
    pub usage: Option<TokenUsage>,
    pub output_tokens_per_second: Option<f64>,
    pub time_to_first_token_ms: Option<f64>,
}

pub fn record_usage(
    config_id: &str,
    usage: Option<TokenUsage>,
    elapsed: Duration,
    first: Option<Duration>,
) {
    let generation = Some(first.and_then(|first| elapsed.checked_sub(first)).unwrap_or(elapsed));
    let record = ModelUsageRecord {
        config_id: config_id.to_string(),
        used_at: Utc::now(),
        output_tokens_per_second: generation.filter(|duration| !duration.is_zero()).and_then(
            |duration| {
                usage
                    .as_ref()?
                    .completion_tokens
                    .map(|tokens| tokens as f64 / duration.as_secs_f64())
            },
        ),
        time_to_first_token_ms: first.map(|duration| duration.as_secs_f64() * 1000.0),
        usage,
    };
    let result = crate::runtime::run_outside_async_runtime(|| {
        crate::system_database::with_database(async |database| {
            let mut connection = database.pool.acquire().await?;
            crate::migration::insert_usage(
                database,
                &mut connection,
                &uuid::Uuid::new_v4().to_string(),
                &record,
            )
            .await
        })
    });
    if let Err(error) = result {
        log::warn!("Failed to persist model usage: {error}");
    }
}

#[derive(Default, Serialize)]
pub struct ModelUsageSummary {
    pub config_id: String,
    pub last_used_at: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub total_tokens: u64,
    pub average_cache_hit_rate: Option<f64>,
    pub average_tokens_per_second: Option<f64>,
    pub average_first_token_ms: Option<f64>,
    pub recent_records: Vec<ModelUsageRecord>,
}

pub fn load_usage_summaries(
    config_ids: &[String],
    range: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> Result<Vec<ModelUsageSummary>> {
    crate::system_database::with_database(async |database| {
        let format_time =
            |date: DateTime<Utc>| date.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
        let start = range.map(|(start, _)| format_time(start));
        let end = range.map(|(_, end)| format_time(end));
        let cutoff = start
            .clone()
            .unwrap_or_else(|| format_time(Utc::now() - chrono::Duration::days(32)));
        let integer = if database.mysql { "SIGNED" } else { "INTEGER" };
        let sql = format!("SELECT COALESCE(MAX(used_at), '') AS last_used_at,
            CAST(COALESCE(SUM(input_tokens), 0) AS {integer}) AS input_tokens,
            CAST(COALESCE(SUM(output_tokens), 0) AS {integer}) AS output_tokens,
            CAST(COALESCE(SUM(cached_input_tokens), 0) AS {integer}) AS cached_input_tokens,
            CAST(COALESCE(SUM(total_tokens), 0) AS {integer}) AS total_tokens,
            COALESCE(AVG(cache_hit_rate), 0.0) AS cache_hit_rate,
            COALESCE(AVG(tokens_per_second), 0.0) AS tokens_per_second,
            COALESCE(AVG(first_token_ms), 0.0) AS first_token_ms,
            CAST(COUNT(cache_hit_rate) AS {integer}) AS cache_samples,
            CAST(COUNT(tokens_per_second) AS {integer}) AS speed_samples,
            CAST(COUNT(first_token_ms) AS {integer}) AS first_token_samples
            FROM zihuan_settings.model_usages WHERE config_id = ? AND (? IS NULL OR used_at >= ?) AND (? IS NULL OR used_at < ?)");
        let mut summaries = Vec::new();
        let mut transaction = database.pool.begin().await?;
        for config_id in config_ids {
            let row = sqlx::query(&sql)
                .bind(config_id)
                .bind(&start)
                .bind(&start)
                .bind(&end)
                .bind(&end)
                .fetch_one(&mut *transaction)
                .await?;
            let records = sqlx::query("SELECT record_json FROM zihuan_settings.model_usages WHERE config_id = ? AND used_at >= ? AND (? IS NULL OR used_at < ?) ORDER BY used_at, id")
                .bind(config_id).bind(&cutoff).bind(&end).bind(&end).fetch_all(&mut *transaction).await?;
            let recent_records = records
                .iter()
                .map(|row| Ok(serde_json::from_slice(row.try_get::<&[u8], _>("record_json")?)?))
                .collect::<Result<Vec<_>>>()?;
            summaries.push(ModelUsageSummary {
                config_id: config_id.clone(),
                last_used_at: Some(row.try_get::<String, _>("last_used_at")?)
                    .filter(|value| !value.is_empty()),
                input_tokens: row.try_get::<i64, _>("input_tokens")? as u64,
                output_tokens: row.try_get::<i64, _>("output_tokens")? as u64,
                cached_input_tokens: row.try_get::<i64, _>("cached_input_tokens")? as u64,
                total_tokens: row.try_get::<i64, _>("total_tokens")? as u64,
                average_cache_hit_rate: (row.try_get::<i64, _>("cache_samples")? > 0)
                    .then(|| row.try_get("cache_hit_rate"))
                    .transpose()?,
                average_tokens_per_second: (row.try_get::<i64, _>("speed_samples")? > 0)
                    .then(|| row.try_get("tokens_per_second"))
                    .transpose()?,
                average_first_token_ms: (row.try_get::<i64, _>("first_token_samples")? > 0)
                    .then(|| row.try_get("first_token_ms"))
                    .transpose()?,
                recent_records,
            });
        }
        transaction.commit().await?;
        Ok(summaries)
    })
}

/// One LLM inference reported to the Data Explorer context view.
///
/// The bubble appears as soon as the inference starts, carrying what is already known (model,
/// the exact request messages, estimated input tokens); streamed responses update it live with
/// the accumulated text and the time to first token, and the finish event replaces its detail
/// with the measured duration and the token usage. Like the embedding stage, the whole run
/// reports under one stable stage id, so start, stream updates and finish fold into one bubble.
///
/// The run only exists when the calling thread is inside a turn's [`crate::qq_context::TurnTraceGuard`]
/// scope; otherwise every constructor returns `None` and inference proceeds untraced.
struct LlmStageRun {
    context: crate::qq_context::TurnTraceContext,
    stage_id: String,
    detail: Value,
    last_stream_emit: Option<Instant>,
}

impl LlmStageRun {
    fn start(
        config_id: &str,
        model: &str,
        param: &InferenceParam,
        streaming: bool,
    ) -> Option<Self> {
        let stage_id = next_llm_stage_id();
        // The payload is only built when a turn is actually being traced, so untracked callers
        // (background jobs, other services) pay nothing for the report.
        with_turn_trace_context(|context| {
            let messages: Vec<Value> = param
                .messages
                .iter()
                .map(|message| {
                    let mut entry = serde_json::Map::new();
                    entry.insert(
                        "role".to_string(),
                        json!(format!("{:?}", message.role).to_lowercase()),
                    );
                    entry.insert("content".to_string(), json!(raw_message_content(message)));
                    if !message.tool_calls.is_empty() {
                        entry.insert("tool_calls".to_string(), json!(message.tool_calls));
                    }
                    if let Some(tool_call_id) = &message.tool_call_id {
                        entry.insert("tool_call_id".to_string(), json!(tool_call_id));
                    }
                    Value::Object(entry)
                })
                .collect();
            let detail = json!({
                "status": "running",
                "model": model,
                "config_id": config_id,
                "streaming": streaming,
                "started_at": Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                "message_count": messages.len(),
                "prompt_tokens_estimated": estimate_messages_tokens(param.messages),
                "messages": messages,
                "tools_count": param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            });
            emit_llm_stage(context, &stage_id, &detail);
            Self {
                context: context.clone(),
                stage_id,
                detail,
                last_stream_emit: None,
            }
        })
    }

    /// Push the streamed text accumulated so far into the bubble, throttled so long replies do
    /// not flood the context feed. The first token reports immediately (time to first token).
    fn stream_update(&mut self, content: &str, reasoning: &str, first: Option<Duration>) {
        if let Some(first) = first {
            self.detail["first_token_ms"] = json!(first.as_secs_f64() * 1000.0);
        }
        if !content.is_empty() {
            self.detail["stream_content"] = json!(content);
        }
        if !reasoning.is_empty() {
            self.detail["stream_reasoning"] = json!(reasoning);
        }
        let now = Instant::now();
        let due = self
            .last_stream_emit
            .is_none_or(|last| now.duration_since(last) >= LLM_STREAM_UPDATE_INTERVAL);
        if !due {
            return;
        }
        self.last_stream_emit = Some(now);
        emit_llm_stage(&self.context, &self.stage_id, &self.detail);
    }

    fn finish(
        mut self,
        elapsed: Duration,
        usage: Option<&TokenUsage>,
        first: Option<Duration>,
        response: &LLMMessage,
    ) {
        self.detail["status"] = json!("done");
        self.detail["duration_ms"] = json!(elapsed.as_millis());
        if let Some(first) = first {
            self.detail["first_token_ms"] = json!(first.as_secs_f64() * 1000.0);
        }
        // The settled response of this call. Non-streaming calls never produce stream_content,
        // and a tool-call iteration's assistant text would otherwise never show anywhere in the
        // flow, so the finished bubble carries the final text and thinking explicitly.
        if let Some(content) = response.content_text() {
            self.detail["content"] = json!(content);
        }
        if let Some(reasoning) = &response.reasoning_content {
            self.detail["reasoning"] = json!(reasoning);
        }
        if let Some(usage) = usage {
            self.detail["usage"] = json!(usage);
            let generation = first.and_then(|first| elapsed.checked_sub(first)).unwrap_or(elapsed);
            if let Some(tokens) = usage.completion_tokens {
                if !generation.is_zero() {
                    self.detail["output_tokens_per_second"] =
                        json!(tokens as f64 / generation.as_secs_f64());
                }
            }
            if let (Some(cached), Some(prompt)) = (usage.cached_prompt_tokens, usage.prompt_tokens)
            {
                if prompt > 0 {
                    self.detail["cache_hit_rate"] = json!(cached as f64 / prompt as f64);
                }
            }
        }
        emit_llm_stage(&self.context, &self.stage_id, &self.detail);
    }

    fn fail(mut self, elapsed: Duration, error: &crate::error::Error) {
        self.detail["status"] = json!("error");
        self.detail["duration_ms"] = json!(elapsed.as_millis());
        self.detail["error"] = json!(error.to_string());
        emit_llm_stage(&self.context, &self.stage_id, &self.detail);
    }
}

fn next_llm_stage_id() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("llm-stage-{seq}")
}

fn emit_llm_stage(context: &crate::qq_context::TurnTraceContext, stage_id: &str, detail: &Value) {
    crate::qq_context::emit_qq_context_stage_with_id(
        &context.agent_id,
        &context.sender_id,
        &context.turn_id,
        QQ_CONTEXT_STAGE_LLM,
        stage_id,
        detail.clone(),
    );
}

#[derive(Debug)]
pub struct TrackedLlm {
    pub config_id: String,
    pub inner: Arc<dyn LLMBase>,
}

impl LLMBase for TrackedLlm {
    fn get_model_name(&self) -> &str {
        self.inner.get_model_name()
    }
    fn context_length(&self) -> usize {
        self.inner.context_length()
    }
    fn api_style(&self) -> Option<&str> {
        self.inner.api_style()
    }
    fn supports_multimodal_input(&self) -> bool {
        self.inner.supports_multimodal_input()
    }
    fn inference(&self, param: &InferenceParam) -> Result<LLMMessage> {
        let started = Instant::now();
        let stage = LlmStageRun::start(&self.config_id, self.get_model_name(), param, false);
        let message = match self.inner.inference(param) {
            Ok(message) => message,
            Err(error) => {
                if let Some(stage) = stage {
                    stage.fail(started.elapsed(), &error);
                }
                return Err(error);
            }
        };
        // Report the result before persisting usage: the context view should not wait on the
        // usage database write.
        if let Some(stage) = stage {
            stage.finish(started.elapsed(), message.usage.as_ref(), None, &message);
        }
        record_usage(&self.config_id, message.usage.clone(), started.elapsed(), None);
        Ok(message)
    }

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        self.inner.as_streaming().map(|_| self as &dyn StreamingLLMBase)
    }
}

impl StreamingLLMBase for TrackedLlm {
    fn inference_streaming<'a>(
        &'a self,
        param: &'a InferenceParam<'a>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LLMMessage> + Send + 'a>> {
        Box::pin(async move {
            let started = Instant::now();
            let mut stage = LlmStageRun::start(&self.config_id, self.get_model_name(), param, true);
            let (relay_tx, mut relay_rx) = mpsc::unbounded_channel::<StreamToken>();
            let inference = self
                .inner
                .as_streaming()
                .expect("streaming model")
                .inference_streaming(param, relay_tx);
            // The relay watches every token before it reaches the caller: it accumulates the
            // streamed text into the context bubble and notes when the first token arrived.
            let relay = async move {
                let mut first = None;
                let mut content = String::new();
                let mut reasoning = String::new();
                while let Some(token) = relay_rx.recv().await {
                    if first.is_none() && !token.as_str().is_empty() {
                        first = Some(started.elapsed());
                    }
                    match &token {
                        StreamToken::Content(delta) => content.push_str(delta),
                        StreamToken::Thinking(delta) => reasoning.push_str(delta),
                    }
                    if let Some(stage) = stage.as_mut() {
                        stage.stream_update(&content, &reasoning, first);
                    }
                    let _ = token_tx.send(token);
                }
                (first, stage)
            };
            let (message, (first, stage)) = tokio::join!(inference, relay);
            // Report the result before persisting usage: the context view should not wait on
            // the usage database write.
            if let Some(stage) = stage {
                stage.finish(started.elapsed(), message.usage.as_ref(), first, &message);
            }
            if message.usage.is_some() || first.is_some() {
                record_usage(&self.config_id, message.usage.clone(), started.elapsed(), first);
            }
            message
        })
    }
}
