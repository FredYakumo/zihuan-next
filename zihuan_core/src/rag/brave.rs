use reqwest::Client;
use serde::Deserialize;
use std::fmt;
use std::time::Duration;

use crate::runtime::block_async;

use super::web_search_engine::{strip_html_tags, WebSearchEngine, WebSearchImage};

pub struct BraveSearch {
    api_token: String,
    timeout: Duration,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: BraveWebResults,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveSearchItem>,
}

#[derive(Debug, Deserialize)]
struct BraveSearchItem {
    title: String,
    url: String,
    description: Option<String>,
    #[serde(default)]
    extra_snippets: Option<Vec<String>>,
}

impl BraveSearch {
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

    async fn search_async(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>> {
        let response = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_token)
            .header("Accept", "application/json")
            .query(&[
                ("q", query),
                ("count", &search_count.to_string()),
                ("extra_snippets", "true"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Brave search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text().await?;
        let parsed: BraveSearchResponse = serde_json::from_str(&body)
            .map_err(|err| crate::error::Error::StringError(format!("Failed to parse Brave search response: {err}")))?;

        Ok(parsed
            .web
            .results
            .into_iter()
            .map(|item| {
                let mut content = item.description.clone().unwrap_or_default();
                if let Some(snippets) = item.extra_snippets {
                    if !snippets.is_empty() {
                        content.push_str("\n额外片段: ");
                        content.push_str(&snippets.join("\n"));
                    }
                }
                format!("标题: {}\n链接: {}\n内容: {}", item.title, item.url, content)
            })
            .collect())
    }

    async fn fetch_url_direct_async(&self, url: &str) -> crate::error::Result<Vec<String>> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (compatible; zihuan-next/1.0)")
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
        Ok(vec![format!("链接: {url}\n内容: {}", strip_html_tags(&body))])
    }
}

impl WebSearchEngine for BraveSearch {
    fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>> {
        block_async(self.search_async(query, search_count))
    }

    fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>> {
        self.fetch_url_direct(url)
    }

    fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>> {
        block_async(self.fetch_url_direct_async(url))
    }

    fn search_images(&self, _query: &str, _max_results: i64) -> crate::error::Result<Vec<WebSearchImage>> {
        Ok(Vec::new())
    }
}

impl fmt::Debug for BraveSearch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BraveSearch")
            .field("api_token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}
