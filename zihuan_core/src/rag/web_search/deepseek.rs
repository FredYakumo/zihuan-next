use reqwest::Client;
use serde::Deserialize;
use std::fmt;
use std::time::Duration;

use crate::runtime::block_async;

use crate::error::Result;
use crate::utils::string_utils::strip_html_tags;

use super::web_search_engine::{WebSearchEngine, WebSearchImage};

pub struct DeepSeekSearch {
    api_token: String,
    timeout: Duration,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct DeepSeekSearchResponse {
    #[serde(default)]
    results: Vec<DeepSeekSearchItem>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekSearchItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeepSeekFetchResponse {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: String,
}

impl DeepSeekSearch {
    pub fn new(api_token: impl Into<String>, timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("failed to build reqwest client");
        Self {
            api_token: api_token.into(),
            timeout,
            client,
        }
    }

    async fn search_async(
        &self,
        query: &str,
        search_count: i64,
    ) -> crate::error::Result<Vec<String>> {
        let response = self
            .client
            .post("https://api.deepseek.com/v1/tools/search")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({
                "query": query,
                "count": search_count.clamp(1, 50),
                "need_content": false,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::string_error!(
                "DeepSeek search request failed with status {}: {}",
                status,
                body
            ));
        }

        let body = response.text().await?;
        let parsed: DeepSeekSearchResponse = serde_json::from_str(&body).map_err(|err| {
            crate::string_error!("Failed to parse DeepSeek search response: {err}")
        })?;

        Ok(parsed
            .results
            .into_iter()
            .map(|item| {
                let content = item
                    .summary
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .or_else(|| item.snippet.as_deref().filter(|value| !value.trim().is_empty()))
                    .or_else(|| item.content.as_deref().filter(|value| !value.trim().is_empty()))
                    .unwrap_or_default();
                let date = item
                    .date
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| format!("\n发布时间: {value}"))
                    .unwrap_or_default();
                format!("标题: {}\n链接: {}{}\n内容: {}", item.title, item.url, date, content)
            })
            .collect())
    }

    async fn extract_url_async(&self, url: &str) -> Result<Vec<String>> {
        let response = self
            .client
            .post("https://api.deepseek.com/v1/tools/fetch")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({ "url": url }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::string_error!(
                "DeepSeek fetch request failed with status {}: {}",
                status,
                body
            ));
        }

        let body = response.text().await?;
        let parsed: DeepSeekFetchResponse = serde_json::from_str(&body).map_err(|err| {
            crate::string_error!("Failed to parse DeepSeek fetch response: {err}")
        })?;

        let title = parsed
            .title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| format!("标题: {value}\n"))
            .unwrap_or_default();
        Ok(vec![format!("{}链接: {url}\n内容: {}", title, parsed.content)])
    }

    async fn fetch_url_direct_async(&self, url: &str) -> Result<Vec<String>> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (compatible; zihuan-next/1.0)")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::string_error!(
                "Direct web request failed with status {}: {}",
                status,
                body
            ));
        }

        let body = response.text().await?;
        Ok(vec![format!("链接: {url}\n内容: {}", strip_html_tags(&body))])
    }
}

impl WebSearchEngine for DeepSeekSearch {
    fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>> {
        block_async(self.search_async(query, search_count))
    }

    fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>> {
        block_async(self.extract_url_async(url))
    }

    fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>> {
        block_async(self.fetch_url_direct_async(url))
    }

    fn search_images(
        &self,
        _query: &str,
        _max_results: i64,
    ) -> crate::error::Result<Vec<WebSearchImage>> {
        Ok(Vec::new())
    }
}

impl fmt::Debug for DeepSeekSearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeepSeekSearch")
            .field("api_token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}
