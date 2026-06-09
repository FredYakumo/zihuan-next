use std::fs::File;
use std::sync::{Arc, Mutex};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_qwen3::ModelWeights;
use log::{info, warn};
use tokenizers::Tokenizer;
use tokio::sync::mpsc;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::{LLMBase, StreamingLLMBase};
use zihuan_core::llm::{InferenceParam, LLMMessage, StreamToken};

use crate::nn::local_candle_embedding::describe_device;
use crate::nn::local_candle_llm_common::{
    build_usage, decode_token_piece, parse_local_response, prepare_prompt, LocalResponseStreamRenderer,
    DEFAULT_MAX_NEW_TOKENS, USER_VISIBLE_REQUEST_ERROR,
};
use crate::nn::local_llm_registry::{get_local_llm_model_info, resolve_model_dir, LocalLlmModelLayout};
use crate::system_config::LlmServiceConfig;

pub fn build_local_candle_gguf_llm(config: LlmServiceConfig) -> Result<Arc<dyn LLMBase>> {
    let model = LocalCandleGgufLlm::load(config)?;
    Ok(Arc::new(model))
}

#[derive(Debug)]
pub struct LocalCandleGgufLlm {
    model_name: String,
    engine: Mutex<LocalCandleGgufLlmEngine>,
}

struct LocalCandleGgufLlmEngine {
    tokenizer: Tokenizer,
    model: ModelWeights,
    device: Device,
    eos_token_ids: Vec<u32>,
}

impl LocalCandleGgufLlm {
    pub fn load(config: LlmServiceConfig) -> Result<Self> {
        let model_info = get_local_llm_model_info(&config.model_name)?;
        if !matches!(model_info.layout, LocalLlmModelLayout::Gguf) {
            return Err(Error::ValidationError(model_info.reason.unwrap_or_else(|| {
                "api_style=candle_gguf requires a GGUF local text model".to_string()
            })));
        }
        if model_info.supports_multimodal_input || config.supports_multimodal_input {
            return Err(Error::ValidationError(
                "local Candle GGUF multimodal runtime is not implemented yet; choose a text GGUF model".to_string(),
            ));
        }
        if !model_info.available {
            return Err(Error::ValidationError(
                model_info.reason.unwrap_or_else(|| "local model is not available".to_string()),
            ));
        }

        let model_dir = resolve_model_dir(&config.model_name)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).map_err(|err| {
            Error::StringError(format!(
                "failed to load tokenizer for local llm '{}' from '{}': {}",
                config.model_name,
                model_dir.join("tokenizer.json").display(),
                err
            ))
        })?;

        let weight_path = model_dir.join(
            model_info
                .weight_file
                .clone()
                .ok_or_else(|| Error::ValidationError("missing GGUF weight file".to_string()))?,
        );
        let device = select_preferred_device(&config.model_name);
        let mut reader = File::open(&weight_path).map_err(|err| {
            Error::StringError(format!(
                "failed to open local llm weights '{}' for '{}': {}",
                weight_path.display(),
                config.model_name,
                err
            ))
        })?;
        let content = gguf_file::Content::read(&mut reader).map_err(|err| {
            Error::StringError(format!(
                "failed to read GGUF metadata for '{}' from '{}': {}",
                config.model_name,
                weight_path.display(),
                err
            ))
        })?;
        let eos_token_ids = extract_eos_token_ids(&content);
        let model = ModelWeights::from_gguf(content, &mut reader, &device).map_err(|err| {
            Error::StringError(format!(
                "failed to load local Candle GGUF model '{}' on device {}: {}",
                config.model_name,
                describe_device(&device),
                err
            ))
        })?;

        info!(
            "Local Candle GGUF LLM '{}' loaded on device {} from '{}'",
            config.model_name,
            describe_device(&device),
            weight_path.display()
        );

        Ok(Self {
            model_name: config.model_name,
            engine: Mutex::new(LocalCandleGgufLlmEngine {
                tokenizer,
                model,
                device,
                eos_token_ids,
            }),
        })
    }

    fn infer_internal(
        &self,
        param: &InferenceParam,
        mut token_sink: Option<&mut dyn FnMut(StreamToken)>,
    ) -> Result<LLMMessage> {
        let (prompt, _) = prepare_prompt(param, false)?;
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| Error::StringError("local candle gguf engine lock poisoned".to_string()))?;
        let prompt_encoding = engine
            .tokenizer
            .encode(prompt, true)
            .map_err(|err| Error::StringError(format!("failed to tokenize local prompt: {err}")))?;
        let mut tokens = prompt_encoding.get_ids().iter().map(|value| *value as u32).collect::<Vec<_>>();
        if tokens.is_empty() {
            return Err(Error::ValidationError("local llm prompt produced no tokens".to_string()));
        }

        let prompt_token_count = tokens.len();
        let mut generated = Vec::new();
        let mut logits_processor = LogitsProcessor::from_sampling(42, Sampling::ArgMax);
        let mut stream_renderer = LocalResponseStreamRenderer::default();
        engine.model.clear_kv_cache();

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let input = if step == 0 {
                Tensor::new(tokens.as_slice(), &engine.device)
            } else {
                Tensor::new(&[*tokens.last().expect("tokens not empty")], &engine.device)
            }
            .and_then(|tensor| tensor.unsqueeze(0))
            .map_err(|err| Error::StringError(format!("failed to build local llm input tensor: {err}")))?;
            let logits = engine
                .model
                .forward(&input, if step == 0 { 0 } else { tokens.len().saturating_sub(1) })
                .map_err(|err| Error::StringError(format!("local candle gguf forward failed: {err}")))?;
            let next_token = logits_processor
                .sample(&logits.squeeze(0).map_err(|err| Error::StringError(format!("failed to squeeze logits: {err}")))?)
                .map_err(|err| Error::StringError(format!("failed to sample local token: {err}")))?;
            if engine.eos_token_ids.contains(&next_token) {
                break;
            }
            tokens.push(next_token);
            generated.push(next_token);

            if let Some(sink) = token_sink.as_mut() {
                if let Some(piece) = decode_token_piece(&engine.tokenizer, next_token) {
                    for token in stream_renderer.push_piece(&piece) {
                        sink(token);
                    }
                }
            }
        }

        if let Some(sink) = token_sink.as_mut() {
            for token in stream_renderer.finish() {
                sink(token);
            }
        }

        let output_text = engine
            .tokenizer
            .decode(&generated, false)
            .map_err(|err| Error::StringError(format!("failed to decode local llm output: {err}")))?;
        let parsed = parse_local_response(&output_text);
        if parsed.saw_tool_call_marker && !parsed.parsed_tool_call {
            warn!(
                "Local Candle GGUF model '{}' produced an invalid CALL_TOOL payload: {}",
                self.model_name, output_text
            );
        }
        let mut message = parsed.into_message();
        message.usage = build_usage(prompt_token_count, generated.len());
        Ok(message)
    }
}

impl std::fmt::Debug for LocalCandleGgufLlmEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCandleGgufLlmEngine")
            .field("device", &describe_device(&self.device))
            .field("eos_token_ids", &self.eos_token_ids)
            .finish()
    }
}

impl LLMBase for LocalCandleGgufLlm {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn api_style(&self) -> Option<&str> {
        Some("candle_gguf")
    }

    fn supports_multimodal_input(&self) -> bool {
        false
    }

    fn inference(&self, param: &InferenceParam) -> LLMMessage {
        match self.infer_internal(param, None) {
            Ok(message) => message,
            Err(err) => {
                warn!("Local Candle GGUF inference failed for '{}': {}", self.model_name, err);
                LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR)
            }
        }
    }

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        Some(self)
    }
}

impl StreamingLLMBase for LocalCandleGgufLlm {
    fn inference_streaming<'a>(
        &'a self,
        param: &'a InferenceParam<'a>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LLMMessage> + Send + 'a>> {
        Box::pin(async move {
            let mut sink = |token: StreamToken| {
                let _ = token_tx.send(token);
            };
            match self.infer_internal(param, Some(&mut sink)) {
                Ok(message) => message,
                Err(err) => {
                    warn!("Local Candle GGUF streaming inference failed for '{}': {}", self.model_name, err);
                    LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR)
                }
            }
        })
    }
}

fn extract_eos_token_ids(content: &gguf_file::Content) -> Vec<u32> {
    let mut ids = Vec::new();
    if let Some(value) = content.metadata.get("tokenizer.ggml.eos_token_id") {
        if let Ok(id) = value.to_u32() {
            ids.push(id);
        }
    }
    if ids.is_empty() {
        ids.push(151645);
    }
    ids
}

fn select_preferred_device(_model_name: &str) -> Device {
    #[cfg(feature = "candle-cuda")]
    if let Ok(device) = Device::new_cuda(0) {
        return device;
    }

    #[cfg(feature = "candle-metal")]
    if let Ok(device) = Device::new_metal(0) {
        return device;
    }

    Device::Cpu
}
