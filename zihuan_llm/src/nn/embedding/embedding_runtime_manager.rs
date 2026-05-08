use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use chrono::Utc;
use log::info;
use tokio::sync::RwLock;
use zihuan_core::connection_manager::{RuntimeConnectionInstanceSummary, RuntimeConnectionStatus};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;

use crate::nn::queued_embedding_model::QueuedEmbeddingModel;
use crate::system_config::{load_llm_refs, LlmRefConfig, ModelRefSpec};

const EMBEDDING_INSTANCE_IDLE_TIMEOUT_SECS: i64 = 15 * 60;
const EMBEDDING_LOG_PREVIEW_CHARS: usize = 80;

fn next_instance_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("embed-{}-{seq}", Utc::now().timestamp_millis())
}

fn text_char_count(text: &str) -> usize {
    text.chars().count()
}

fn preview_text(text: &str) -> String {
    let normalized = text.replace('\n', "\\n").replace('\r', "\\r");
    let mut chars = normalized.chars();
    let preview: String = chars.by_ref().take(EMBEDDING_LOG_PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

#[derive(Clone)]
struct EmbeddingRuntimeInstance {
    summary: RuntimeConnectionInstanceSummary,
    model: Arc<dyn EmbeddingBase>,
}

#[derive(Debug)]
struct LoggedEmbeddingModel {
    instance_id: String,
    inner: Arc<dyn EmbeddingBase>,
}

impl LoggedEmbeddingModel {
    fn new(instance_id: String, inner: Arc<dyn EmbeddingBase>) -> Self {
        Self { instance_id, inner }
    }
}

impl EmbeddingBase for LoggedEmbeddingModel {
    fn get_model_name(&self) -> &str {
        self.inner.get_model_name()
    }

    fn inference(&self, text: &str) -> Result<Vec<f32>> {
        let started = Instant::now();
        let vector = self.inner.inference(text)?;
        info!(
            "[embedding_runtime] instance_id={} model={} input_count=1 input_chars={} preview=\"{}\" inference_ms={} shape=[{}]",
            self.instance_id,
            self.inner.get_model_name(),
            text_char_count(text),
            preview_text(text),
            started.elapsed().as_millis(),
            vector.len()
        );
        Ok(vector)
    }

    fn batch_inference(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let started = Instant::now();
        let vectors = self.inner.batch_inference(texts)?;
        let dim = vectors.first().map(Vec::len).unwrap_or(0);
        let total_chars = texts.iter().map(|text| text_char_count(text)).sum::<usize>();
        let preview = texts
            .first()
            .map(|text| preview_text(text))
            .unwrap_or_default();
        info!(
            "[embedding_runtime] instance_id={} model={} input_count={} input_chars={} preview=\"{}\" batch_inference_ms={} shape=[{}, {}]",
            self.instance_id,
            self.inner.get_model_name(),
            texts.len(),
            total_chars,
            preview,
            started.elapsed().as_millis(),
            vectors.len(),
            dim
        );
        Ok(vectors)
    }
}

pub struct RuntimeEmbeddingModelManager {
    instances: RwLock<HashMap<String, Vec<EmbeddingRuntimeInstance>>>,
}

impl RuntimeEmbeddingModelManager {
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(HashMap::new()),
        }
    }

    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<RuntimeEmbeddingModelManager> = OnceLock::new();
        INSTANCE.get_or_init(RuntimeEmbeddingModelManager::new)
    }

    pub async fn get_or_create_embedding_model(
        &self,
        config_id: &str,
    ) -> Result<Arc<dyn EmbeddingBase>> {
        self.get_or_create(config_id).await
    }

    fn build_runtime_instance(
        &self,
        config_id: &str,
    ) -> Result<(EmbeddingRuntimeInstance, Arc<dyn EmbeddingBase>)> {
        let llm_refs = load_llm_refs()?;
        let llm_ref = llm_refs
            .iter()
            .find(|item| item.id == config_id)
            .ok_or_else(|| Error::ValidationError(format!("model_ref '{}' not found", config_id)))?;

        if !llm_ref.enabled {
            return Err(Error::ValidationError(format!(
                "model_ref '{}' is disabled",
                llm_ref.name
            )));
        }

        let model_name = local_model_name(llm_ref)?;
        let inner_model: Arc<dyn EmbeddingBase> = Arc::new(QueuedEmbeddingModel::new(model_name)?);
        let started_at = Utc::now();
        let instance_id = next_instance_id();
        let model: Arc<dyn EmbeddingBase> = Arc::new(LoggedEmbeddingModel::new(
            instance_id.clone(),
            inner_model,
        ));
        let summary = RuntimeConnectionInstanceSummary {
            instance_id,
            config_id: llm_ref.id.clone(),
            name: llm_ref.name.clone(),
            kind: "text_embedding_local".to_string(),
            keep_alive: false,
            heartbeat_interval_secs: None,
            started_at,
            last_used_at: started_at,
            status: RuntimeConnectionStatus::Running,
        };
        let handle = Arc::clone(&model);
        Ok((EmbeddingRuntimeInstance { summary, model }, handle))
    }

    async fn mark_used_and_clone(
        &self,
        config_id: &str,
        instances: &mut HashMap<String, Vec<EmbeddingRuntimeInstance>>,
    ) -> Option<Arc<dyn EmbeddingBase>> {
        let bucket = instances.get_mut(config_id)?;
        let first = bucket.first_mut()?;
        first.summary.last_used_at = Utc::now();
        Some(Arc::clone(&first.model))
    }

    async fn get_or_create(&self, config_id: &str) -> Result<Arc<dyn EmbeddingBase>> {
        let config_id = config_id.to_string();
        self.cleanup_stale_instances().await?;
        {
            let mut instances = self.instances.write().await;
            if let Some(handle) = self.mark_used_and_clone(&config_id, &mut instances).await {
                return Ok(handle);
            }
        }

        let (instance, handle) = self.build_runtime_instance(&config_id)?;
        let mut instances = self.instances.write().await;
        instances.entry(config_id).or_default().push(instance);
        Ok(handle)
    }

    async fn list_instances(&self) -> Result<Vec<RuntimeConnectionInstanceSummary>> {
        self.cleanup_stale_instances().await?;
        let instances = self.instances.read().await;
        let mut items = instances
            .values()
            .flat_map(|bucket| bucket.iter().map(|item| item.summary.clone()))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(items)
    }

    async fn close_instance(&self, instance_id: &str) -> Result<bool> {
        let instance_id = instance_id.to_string();
        let mut instances = self.instances.write().await;
        for bucket in instances.values_mut() {
            if let Some(index) = bucket
                .iter()
                .position(|item| item.summary.instance_id == instance_id)
            {
                bucket.remove(index);
                instances.retain(|_, bucket| !bucket.is_empty());
                return Ok(true);
            }
        }
        instances.retain(|_, bucket| !bucket.is_empty());
        Ok(false)
    }

    async fn close_instances_for_config(&self, config_id: &str) -> Result<usize> {
        let config_id = config_id.to_string();
        let mut instances = self.instances.write().await;
        Ok(instances
            .remove(&config_id)
            .map(|items| items.len())
            .unwrap_or(0))
    }

    async fn cleanup_stale_instances(&self) -> Result<usize> {
        let llm_refs = load_llm_refs()?;
        let now = Utc::now();
        let mut instances = self.instances.write().await;
        let mut removed = 0usize;

        for (config_id, bucket) in instances.iter_mut() {
            let enabled = llm_refs
                .iter()
                .find(|item| item.id == *config_id)
                .map(|item| {
                    item.enabled
                        && matches!(item.model, ModelRefSpec::TextEmbeddingLocal { .. })
                })
                .unwrap_or(false);

            let mut retained = Vec::new();
            for item in bucket.drain(..) {
                let stale = (now - item.summary.last_used_at).num_seconds()
                    >= EMBEDDING_INSTANCE_IDLE_TIMEOUT_SECS;
                if enabled && !stale {
                    retained.push(item);
                } else {
                    removed += 1;
                }
            }
            *bucket = retained;
        }

        instances.retain(|_, bucket| !bucket.is_empty());
        Ok(removed)
    }
}

fn local_model_name(llm_ref: &LlmRefConfig) -> Result<String> {
    match &llm_ref.model {
        ModelRefSpec::TextEmbeddingLocal { model_name } if !model_name.trim().is_empty() => {
            Ok(model_name.trim().to_string())
        }
        ModelRefSpec::TextEmbeddingLocal { .. } => Err(Error::ValidationError(format!(
            "embedding model_ref '{}' has empty model_name",
            llm_ref.name
        ))),
        ModelRefSpec::ChatLlm { .. } => Err(Error::ValidationError(format!(
            "model_ref '{}' is not a text_embedding_local config",
            llm_ref.name
        ))),
    }
}

pub fn list_runtime_embedding_instances() -> Result<Vec<RuntimeConnectionInstanceSummary>> {
    zihuan_core::runtime::block_async(RuntimeEmbeddingModelManager::shared().list_instances())
}

pub fn close_runtime_embedding_instance(instance_id: &str) -> Result<bool> {
    zihuan_core::runtime::block_async(
        RuntimeEmbeddingModelManager::shared().close_instance(instance_id),
    )
}

pub fn close_runtime_embedding_instances_for_config(config_id: &str) -> Result<usize> {
    zihuan_core::runtime::block_async(
        RuntimeEmbeddingModelManager::shared().close_instances_for_config(config_id),
    )
}
