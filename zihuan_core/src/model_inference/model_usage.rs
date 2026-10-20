use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tokio::sync::mpsc;

use super::llm::llm_base::{LLMBase, StreamingLLMBase};
use super::llm::{InferenceParam, LLMMessage, StreamToken, TokenUsage};
use crate::error::Result;

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
        let message = self.inner.inference(param)?;
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
            let (relay_tx, mut relay_rx) = mpsc::unbounded_channel::<StreamToken>();
            let inference = self
                .inner
                .as_streaming()
                .expect("streaming model")
                .inference_streaming(param, relay_tx);
            let relay = async move {
                let mut first = None;
                while let Some(token) = relay_rx.recv().await {
                    if first.is_none() && !token.as_str().is_empty() {
                        first = Some(started.elapsed());
                    }
                    let _ = token_tx.send(token);
                }
                first
            };
            let (message, first) = tokio::join!(inference, relay);
            if message.usage.is_some() || first.is_some() {
                record_usage(&self.config_id, message.usage.clone(), started.elapsed(), first);
            }
            message
        })
    }
}
