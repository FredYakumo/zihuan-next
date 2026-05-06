use log::{error, warn};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;

const DEFAULT_RETRY_COUNT: u32 = 2;
const RETRY_DELAY_MS: u64 = 1_000;

#[derive(Debug, Clone)]
pub struct EmbeddingAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    timeout: Duration,
    retry_count: u32,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

impl EmbeddingAPI {
    pub fn new(
        model_name: String,
        api_endpoint: String,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            model_name,
            api_endpoint,
            api_key,
            timeout,
            retry_count: DEFAULT_RETRY_COUNT,
        }
    }

    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }

    fn execute_embedding_request(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let client = Client::builder().timeout(self.timeout).build()?;
        let attempts = self.retry_count.max(1);
        let mut last_error: Option<Error> = None;

        for attempt in 1..=attempts {
            match self.try_execute_embedding_request(&client, texts) {
                Ok(vectors) => return Ok(vectors),
                Err(error) => {
                    let should_retry = attempt < attempts;
                    if should_retry {
                        warn!(
                            "[EmbeddingAPI] request failed on attempt {attempt}/{attempts}: {error}"
                        );
                        std::thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        error!(
                            "[EmbeddingAPI] request failed on final attempt {attempt}/{attempts}: {error}"
                        );
                    }
                    last_error = Some(error);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            Error::StringError("embedding request failed without a concrete error".to_string())
        }))
    }

    fn try_execute_embedding_request(
        &self,
        client: &Client,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let input = if texts.len() == 1 {
            json!(texts[0])
        } else {
            json!(texts)
        };

        let mut request = client.post(&self.api_endpoint).json(&json!({
            "model": self.model_name,
            "input": input,
        }));

        if let Some(api_key) = &self.api_key {
            let auth_header = if api_key.starts_with("Bearer ") {
                api_key.clone()
            } else {
                format!("Bearer {api_key}")
            };
            request = request.header("Authorization", auth_header);
        }

        let response = request.send()?;
        let status = response.status();
        let body = response.text()?;

        if !status.is_success() {
            return Err(Error::StringError(format!(
                "embedding request failed with status {}: {}",
                status, body
            )));
        }

        let mut parsed: EmbeddingResponse = serde_json::from_str(&body)?;
        parsed.data.sort_by_key(|item| item.index);
        Ok(parsed.data.into_iter().map(|item| item.embedding).collect())
    }
}

impl EmbeddingBase for EmbeddingAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::ValidationError(
                "text must not be blank when requesting embeddings".to_string(),
            ));
        }

        let mut vectors = self.execute_embedding_request(&[text.to_string()])?;
        vectors.pop().ok_or_else(|| {
            Error::StringError("embedding API returned an empty data list".to_string())
        })
    }

    fn batch_inference(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(Error::ValidationError(
                "texts must not be empty when requesting embeddings".to_string(),
            ));
        }
        if texts.iter().any(|text| text.trim().is_empty()) {
            return Err(Error::ValidationError(
                "texts must not contain blank items when requesting embeddings".to_string(),
            ));
        }

        self.execute_embedding_request(texts)
    }
}
