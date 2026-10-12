use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::llm::llm_base::{LLMBase, StreamingLLMBase};
use super::llm::{InferenceParam, LLMMessage, StreamToken, TokenUsage};
use crate::error::Result;

static USAGE_LOCK: Mutex<()> = Mutex::new(());

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
    let result = (|| -> Result<()> {
        let _guard = USAGE_LOCK
            .lock()
            .map_err(|_| crate::string_error!("model usage lock poisoned"))?;
        let directory = crate::system_config::application_data_dir().join("model_usage");
        fs::create_dir_all(&directory)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("usage.jsonl"))?;
        serde_json::to_writer(&mut file, &record)?;
        file.write_all(b"\n")?;
        Ok(())
    })();
    if let Err(error) = result {
        log::warn!("Failed to persist model usage: {error}");
    }
}

pub fn load_usage() -> Result<Vec<ModelUsageRecord>> {
    let _guard = USAGE_LOCK
        .lock()
        .map_err(|_| crate::string_error!("model usage lock poisoned"))?;
    let path = crate::system_config::application_data_dir().join("model_usage/usage.jsonl");
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    BufReader::new(file)
        .lines()
        .map(|line| Ok(serde_json::from_str(&line?)?))
        .collect()
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
