use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use candle_core::{safetensors, DType, Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::qwen3::{Config, Model};
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;

const LOCAL_MODEL_ROOT: &str = "models/text_embedding";

pub struct LocalCandleEmbeddingModel {
    model_name: String,
    model_dir: PathBuf,
    config: Config,
    tokenizer: Tokenizer,
    max_length: usize,
    preferred_device: Device,
}

impl LocalCandleEmbeddingModel {
    pub fn load(model_name: &str) -> Result<Self> {
        let model_dir = resolve_model_dir(model_name)?;
        let config_path = model_dir.join("config.json");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let config_text = fs::read_to_string(&config_path).map_err(|err| {
            Error::StringError(format!(
                "failed to read local embedding config '{}' for model '{}': {}",
                config_path.display(),
                model_name,
                err
            ))
        })?;
        let config: Config = serde_json::from_str(&config_text).map_err(|err| {
            Error::StringError(format!(
                "failed to parse local embedding config '{}' for model '{}': {}",
                config_path.display(),
                model_name,
                err
            ))
        })?;
        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|err| {
            Error::StringError(format!(
                "failed to load local embedding tokenizer '{}' for model '{}' (model_dir='{}', cwd='{}'): {}",
                tokenizer_path.display(),
                model_name,
                model_dir.display(),
                current_dir_display(),
                err
            ))
        })?;

        let preferred_device = select_preferred_device(model_name);
        log::info!(
            "Local embedding model '{}' loaded with device type: {}",
            model_name,
            describe_device(&preferred_device)
        );

        Ok(Self {
            model_name: model_name.to_string(),
            model_dir,
            config: config.clone(),
            tokenizer,
            max_length: config.max_position_embeddings,
            preferred_device,
        })
    }

    fn load_runtime_model(&self, device: &Device) -> Result<Model> {
        let dtype = device.bf16_default_to_f32();
        let tensors = safetensors::load(self.model_dir.join("model.safetensors"), device)
            .map_err(|err| {
                Error::StringError(format!(
                    "failed to load local embedding weights '{}' for model '{}': {}",
                    self.model_dir.join("model.safetensors").display(),
                    self.model_name,
                    err
                ))
            })?;
        let tensors: HashMap<String, Tensor> = tensors
            .into_iter()
            .map(|(name, tensor)| (format!("model.{name}"), tensor))
            .collect();
        let vb = VarBuilder::from_tensors(tensors, dtype, device);
        Model::new(&self.config, vb).map_err(|err| {
            Error::StringError(format!(
                "failed to load Candle Qwen3 model for '{}': {}",
                self.model_name, err
            ))
        })
    }

    fn encode_batch(&self, texts: &[String], device: &Device) -> Result<(Tensor, Vec<usize>)> {
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
        let input_ids = Tensor::from_vec(tokens, (texts.len(), max_len), device)
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

    fn infer_batch_on_device(&self, texts: &[String], device: &Device) -> Result<Vec<Vec<f32>>> {
        let (input_ids, lengths) = self.encode_batch(texts, device)?;
        let mut model = self.load_runtime_model(device)?;
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

    fn infer_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        match self.infer_batch_on_device(texts, &self.preferred_device) {
            Ok(embeddings) => Ok(embeddings),
            Err(err) if !self.preferred_device.is_cpu() => {
                log::warn!(
                    "Local embedding model '{}' failed on preferred device {}; falling back to CPU: {}",
                    self.model_name,
                    describe_device(&self.preferred_device),
                    err
                );
                self.infer_batch_on_device(texts, &Device::Cpu)
            }
            Err(err) => Err(err),
        }
    }
}

impl std::fmt::Debug for LocalCandleEmbeddingModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCandleEmbeddingModel")
            .field("model_name", &self.model_name)
            .field("model_dir", &self.model_dir)
            .field("max_length", &self.max_length)
            .field("preferred_device", &describe_device(&self.preferred_device))
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

    let model_root = Path::new(LOCAL_MODEL_ROOT);
    let model_dir = model_root.join(trimmed);
    if !model_dir.is_dir() {
        let available = available_local_models()?;
        let available_hint = if available.is_empty() {
            "no local models found".to_string()
        } else {
            format!("available models: {}", available.join(", "))
        };
        return Err(Error::ValidationError(format!(
            "local embedding model '{}' was not found.\nexpected_dir='{}'\nmodel_root='{}'\ncwd='{}'\n{}",
            trimmed,
            display_path(&model_dir),
            display_path(model_root),
            current_dir_display(),
            available_hint
        )));
    }

    for required in ["config.json", "tokenizer.json", "model.safetensors"] {
        let path = model_dir.join(required);
        if !path.is_file() {
            return Err(Error::ValidationError(format!(
                "local embedding model '{}' is missing required file '{}'\nmodel_dir='{}'\ncwd='{}'",
                trimmed,
                path.display(),
                display_path(&model_dir),
                current_dir_display()
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

fn current_dir_display() -> String {
    std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string())
}

fn display_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn select_preferred_device(model_name: &str) -> Device {
    #[cfg(feature = "candle-cuda")]
    {
        match Device::new_cuda(0) {
            Ok(device) => {
                log::info!(
                    "Local embedding model '{}' will use CUDA device 0",
                    model_name
                );
                return device;
            }
            Err(err) => {
                log::warn!(
                    "CUDA was enabled for local embedding model '{}' but unavailable at runtime: {}",
                    model_name,
                    err
                );
            }
        }
    }

    #[cfg(all(feature = "candle-metal", target_os = "macos"))]
    {
        match Device::new_metal(0) {
            Ok(device) => {
                log::info!(
                    "Local embedding model '{}' will use Metal device 0",
                    model_name
                );
                return device;
            }
            Err(err) => {
                log::warn!(
                    "Metal was enabled for local embedding model '{}' but unavailable at runtime: {}",
                    model_name,
                    err
                );
            }
        }
    }

    log::info!(
        "Local embedding model '{}' will use CPU fallback",
        model_name
    );
    Device::Cpu
}

fn describe_device(device: &Device) -> &'static str {
    if device.is_cuda() {
        "cuda"
    } else if device.is_metal() {
        "metal"
    } else {
        "cpu"
    }
}
