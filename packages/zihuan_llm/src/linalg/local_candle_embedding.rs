use std::fs;
use std::path::{Path, PathBuf};

use candle_core::{DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen3::{Config, Model};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use zihuan_core::error::{Error, Result};
use zihuan_llm_types::embedding_base::EmbeddingBase;

const LOCAL_MODEL_ROOT: &str = "models/text_embedding";

pub struct LocalCandleEmbeddingModel {
    model_name: String,
    model_dir: PathBuf,
    config: Config,
    tokenizer: Tokenizer,
    max_length: usize,
}

impl LocalCandleEmbeddingModel {
    pub fn load(model_name: &str) -> Result<Self> {
        let model_dir = resolve_model_dir(model_name)?;
        let config: Config =
            serde_json::from_str(&fs::read_to_string(model_dir.join("config.json"))?)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|err| Error::StringError(format!("failed to load tokenizer: {err}")))?;

        Ok(Self {
            model_name: model_name.to_string(),
            model_dir,
            config: config.clone(),
            tokenizer,
            max_length: config.max_position_embeddings,
        })
    }

    fn load_runtime_model(&self) -> Result<Model> {
        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[self.model_dir.join("model.safetensors")],
                DType::BF16,
                &device,
            )
        }
        .map_err(|err| Error::StringError(format!("failed to map model weights: {err}")))?;
        Model::new(&self.config, vb)
            .map_err(|err| Error::StringError(format!("failed to load Candle Qwen3 model: {err}")))
    }

    fn encode_batch(&self, texts: &[String]) -> Result<(Tensor, Vec<usize>)> {
        let mut tokenizer = self.tokenizer.clone();
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            ..Default::default()
        }));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: self.max_length.min(32768),
                ..Default::default()
            }))
            .map_err(|err| {
                Error::StringError(format!("failed to enable tokenizer truncation: {err}"))
            })?;

        let encodings = tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|err| Error::StringError(format!("failed to tokenize inputs: {err}")))?;

        let token_rows = encodings
            .iter()
            .map(|encoding| {
                encoding
                    .get_ids()
                    .iter()
                    .map(|id| *id as u32)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let lengths = encodings
            .iter()
            .map(|encoding| {
                encoding
                    .get_attention_mask()
                    .iter()
                    .filter(|&&flag| flag != 0)
                    .count()
            })
            .collect::<Vec<_>>();

        let max_len = token_rows.iter().map(Vec::len).max().unwrap_or(0);
        if max_len == 0 {
            return Err(Error::ValidationError(
                "texts input must not be empty".to_string(),
            ));
        }

        let tokens = token_rows.concat();
        let input_ids = Tensor::from_vec(tokens, (texts.len(), max_len), &Device::Cpu)
            .map_err(|err| Error::StringError(format!("failed to build token tensor: {err}")))?;

        Ok((input_ids, lengths))
    }

    fn normalize_embedding(values: &mut [f32]) {
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in values {
                *value /= norm;
            }
        }
    }

    fn infer_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let (input_ids, lengths) = self.encode_batch(texts)?;
        let mut model = self.load_runtime_model()?;
        let hidden_states = model
            .forward(&input_ids, 0)
            .map_err(|err| Error::StringError(format!("Candle embedding forward failed: {err}")))?;

        let mut embeddings = Vec::with_capacity(lengths.len());
        for (batch_index, token_len) in lengths.into_iter().enumerate() {
            let token_index = token_len.saturating_sub(1);
            let embedding = hidden_states
                .i((batch_index, token_index))
                .and_then(|tensor| tensor.to_dtype(DType::F32))
                .and_then(|tensor| tensor.to_vec1::<f32>())
                .map_err(|err| {
                    Error::StringError(format!("failed to read embedding tensor: {err}"))
                })?;
            let mut embedding = embedding;
            Self::normalize_embedding(&mut embedding);
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}

impl std::fmt::Debug for LocalCandleEmbeddingModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCandleEmbeddingModel")
            .field("model_name", &self.model_name)
            .field("model_dir", &self.model_dir)
            .field("max_length", &self.max_length)
            .finish()
    }
}

impl EmbeddingBase for LocalCandleEmbeddingModel {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::ValidationError(
                "text input must not be blank".to_string(),
            ));
        }
        let texts = vec![text.to_string()];
        let mut embeddings = self.infer_batch(&texts)?;
        embeddings
            .pop()
            .ok_or_else(|| Error::StringError("embedding model returned no vectors".to_string()))
    }

    fn batch_inference(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(Error::ValidationError(
                "texts input must not be empty".to_string(),
            ));
        }
        self.infer_batch(texts)
    }
}

pub fn resolve_model_dir(model_name: &str) -> Result<PathBuf> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "model_name is required for local embedding models".to_string(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(Error::ValidationError(format!(
            "model_name must be a direct child directory under {LOCAL_MODEL_ROOT}"
        )));
    }

    let model_dir = Path::new(LOCAL_MODEL_ROOT).join(trimmed);
    if !model_dir.is_dir() {
        let available = available_local_models()?;
        let available_hint = if available.is_empty() {
            "no local models found".to_string()
        } else {
            format!("available models: {}", available.join(", "))
        };
        return Err(Error::ValidationError(format!(
            "local embedding model '{}' was not found under {} ({available_hint})",
            trimmed, LOCAL_MODEL_ROOT
        )));
    }

    for required in ["config.json", "tokenizer.json", "model.safetensors"] {
        let path = model_dir.join(required);
        if !path.is_file() {
            return Err(Error::ValidationError(format!(
                "local embedding model '{}' is missing required file {}",
                trimmed,
                path.display()
            )));
        }
    }

    Ok(model_dir)
}

fn available_local_models() -> Result<Vec<String>> {
    let root = Path::new(LOCAL_MODEL_ROOT);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut names = fs::read_dir(root)?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}
