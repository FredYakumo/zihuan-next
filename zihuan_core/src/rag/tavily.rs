use reqwest::Client;
use serde::Deserialize;
use std::fmt;
use std::time::Duration;

use crate::runtime::block_async;

use super::web_search_engine::{strip_html_tags, WebSearchEngine, WebSearchImage};

pub struct TavilySearch {
    api_token: String,
    timeout: Duration,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct TavilySearchResponse {
    results: Vec<TavilySearchItem>,
}

#[derive(Debug, Deserialize)]
struct TavilySearchItem {
    title: String,
    url: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct TavilyExtractResponse {
    #[serde(default)]
    results: Vec<TavilyExtractItem>,
    #[serde(default)]
    failed_results: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TavilyExtractItem {
    url: String,
    raw_content: String,
}

#[derive(Debug, Deserialize)]
struct TavilyImageSearchResponse {
    #[serde(default)]
    images: Vec<WebSearchImage>,
}

impl TavilySearch {
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
            .post("https://api.tavily.com/search")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({
                "query": query,
                "max_results": search_count,
                "search_depth": "advanced",
                "include_answer": false,
                "include_images": false,
                "include_raw_content": false,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text().await?;
        let parsed: TavilySearchResponse = serde_json::from_str(&body).map_err(|err| {
            crate::error::Error::StringError(format!(
                "Failed to parse Tavily search response: {err}"
            ))
        })?;

        Ok(parsed
            .results
            .into_iter()
            .map(|item| {
                format!(
                    "标题: {}\n链接: {}\n内容: {}",
                    item.title, item.url, item.content
                )
            })
            .collect())
    }

    async fn extract_url_async(&self, url: &str) -> crate::error::Result<Vec<String>> {
        let response = self
            .client
            .post("https://api.tavily.com/extract")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({
                "urls": url,
                "extract_depth": "advanced",
                "format": "markdown",
                "include_images": false,
                "include_favicon": false,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily extract request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text().await?;
        let parsed: TavilyExtractResponse = serde_json::from_str(&body).map_err(|err| {
            crate::error::Error::StringError(format!(
                "Failed to parse Tavily extract response: {err}"
            ))
        })?;

        if parsed.results.is_empty() {
            return Err(crate::error::Error::StringError(format!(
                "Tavily extract returned no successful results: {:?}",
                parsed.failed_results
            )));
        }

        Ok(parsed
            .results
            .into_iter()
            .map(|item| format!("链接: {}\n内容: {}", item.url, item.raw_content))
            .collect())
    }

    async fn fetch_url_direct_async(&self, url: &str) -> crate::error::Result<Vec<String>> {
        let response = self
            .client
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (compatible; zihuan-next/1.0)",
            )
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Direct web request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text().await?;
        Ok(vec![format!(
            "链接: {url}\n内容: {}",
            strip_html_tags(&body)
        )])
    }

    async fn search_images_async(
        &self,
        query: &str,
        max_results: i64,
    ) -> crate::error::Result<Vec<WebSearchImage>> {
        let response = self
            .client
            .post("https://api.tavily.com/search")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({
                "query": query,
                "max_results": max_results,
                "search_depth": "basic",
                "include_answer": false,
                "include_images": true,
                "include_image_descriptions": true,
                "include_raw_content": false,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text().await?;
        let parsed: TavilyImageSearchResponse = serde_json::from_str(&body).map_err(|err| {
            crate::error::Error::StringError(format!(
                "Failed to parse Tavily search response: {err}"
            ))
        })?;

        Ok(parsed.images)
    }
}

impl WebSearchEngine for TavilySearch {
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
        query: &str,
        max_results: i64,
    ) -> crate::error::Result<Vec<WebSearchImage>> {
        block_async(self.search_images_async(query, max_results))
    }
}

impl fmt::Debug for TavilySearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TavilySearch")
            .field("api_token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}
