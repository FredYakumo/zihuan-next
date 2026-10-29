//! Deterministic LLM implementation. It consumes real role prompts and rejects unknown ones.
use super::llm::llm_base::LLMBase;
use super::llm::{InferenceParam, LLMMessage, TokenUsage};
use super::model_config::LlmServiceConfig;
use crate::error::Result;
use std::path::PathBuf;

#[derive(Debug)]
pub struct MockLlm {
    name: String,
    context_length: usize,
    trace_directory: PathBuf,
}

impl MockLlm {
    pub fn new(config: &LlmServiceConfig) -> Self {
        Self {
            name: config.model_name.clone(),
            context_length: config.context_length,
            trace_directory: PathBuf::from(&config.api_endpoint),
        }
    }
}

impl LLMBase for MockLlm {
    fn get_model_name(&self) -> &str {
        &self.name
    }
    fn context_length(&self) -> usize {
        self.context_length
    }
    fn inference(&self, param: &InferenceParam) -> Result<LLMMessage> {
        let texts: Vec<String> = param
            .messages
            .iter()
            .map(|m| m.content_text_owned().unwrap_or_default())
            .collect();
        let system = texts.first().map(String::as_str).unwrap_or_default();
        let current = texts.last().map(String::as_str).unwrap_or_default();
        let (kind, output) = if system.contains("message intent classifier") {
            ("intent", "Chat".to_string())
        } else if system.contains("you must select the history texts") {
            let window = current.split("[Group Window Messages]").nth(1).unwrap_or_default();
            let indexes: Vec<usize> = if current.contains("用户A喜欢什么角色") {
                window
                    .lines()
                    .filter(|line| line.contains("用户A(") && line.contains("我喜欢"))
                    .filter_map(|line| line.strip_prefix('[')?.split(']').next()?.parse().ok())
                    .collect()
            } else {
                Vec::new()
            };
            ("group_window", serde_json::json!({"related": indexes}).to_string())
        } else if system.contains("rewritten_message") && system.contains("safe") {
            (
                "review",
                serde_json::json!({"safe": true, "rewritten_message": "", "reason": "mock review"})
                    .to_string(),
            )
        } else if current.contains("世界上最好的二次元游戏是什么") {
            ("brain", "原神".to_string())
        } else if current.contains("用户A喜欢什么角色") {
            // Only read the history supplied by the procedure, never the candidate window.
            let history = texts
                .iter()
                .take(texts.len().saturating_sub(1))
                .find(|text| text.starts_with("Here are the group messages related"))
                .ok_or_else(|| crate::string_error!("mock brain: group window history missing"))?;
            let line = history
                .lines()
                .find(|line| line.contains("用户A(") && line.contains("我喜欢"))
                .ok_or_else(|| crate::string_error!("mock brain: user A preference missing"))?;
            let answer = line.split_once("我喜欢").map(|(_, tail)| tail.trim()).unwrap_or_default();
            ("brain", answer.to_string())
        } else {
            return Err(crate::string_error!(
                "mock LLM received an unsupported prompt: {}",
                current
            ));
        };
        let mut response = LLMMessage::assistant_text(output.clone());
        let prompt_tokens = texts.iter().map(|s| s.chars().count()).sum();
        let completion_tokens = output.chars().count();
        response.usage = Some(TokenUsage {
            prompt_tokens: Some(prompt_tokens),
            completion_tokens: Some(completion_tokens),
            total_tokens: Some(prompt_tokens + completion_tokens),
            ..Default::default()
        });
        std::fs::create_dir_all(&self.trace_directory)?;
        crate::utils::json_file::write_json_atomic(
            &self.trace_directory.join(format!("{}.json", uuid::Uuid::new_v4())),
            &serde_json::json!({"kind": kind, "messages": texts, "output": output}),
        )?;
        Ok(response)
    }
}
