use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::time::Duration;

#[derive(Clone)]
pub struct TavilyRef {
    pub api_token: String,
    pub timeout: Duration,
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
    failed_results: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct TavilyExtractItem {
    url: String,
    raw_content: String,
}

#[derive(Debug, Deserialize)]
pub struct TavilyImage {
    pub url: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TavilyImageSearchResponse {
    #[serde(default)]
    images: Vec<TavilyImage>,
}

impl TavilyRef {
    pub fn new(api_token: impl Into<String>, timeout: Duration) -> Self {
        Self {
            api_token: api_token.into(),
            timeout,
        }
    }

    pub fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>> {
        let client = Client::builder().timeout(self.timeout).build()?;
        let response = client
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
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text()?;
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

    pub fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>> {
        let client = Client::builder().timeout(self.timeout).build()?;
        let response = client
            .post("https://api.tavily.com/extract")
            .bearer_auth(&self.api_token)
            .json(&serde_json::json!({
                "urls": url,
                "extract_depth": "advanced",
                "format": "markdown",
                "include_images": false,
                "include_favicon": false,
            }))
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily extract request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text()?;
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

    pub fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>> {
        let client = Client::builder().timeout(self.timeout).build()?;
        let response = client
            .get(url)
            .header(
                reqwest::header::USER_AGENT,
                "Mozilla/5.0 (compatible; zihuan-next/1.0)",
            )
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Direct web request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text()?;
        Ok(vec![format!(
            "链接: {url}\n内容: {}",
            strip_html_tags(&body)
        )])
    }

    pub fn search_images(
        &self,
        query: &str,
        max_results: i64,
    ) -> crate::error::Result<Vec<TavilyImage>> {
        let client = Client::builder().timeout(self.timeout).build()?;
        let response = client
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
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(crate::error::Error::StringError(format!(
                "Tavily search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text()?;
        let parsed: TavilyImageSearchResponse = serde_json::from_str(&body).map_err(|err| {
            crate::error::Error::StringError(format!(
                "Failed to parse Tavily search response: {err}"
            ))
        })?;

        Ok(parsed.images)
    }
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    let mut previous_was_whitespace = false;

    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !previous_was_whitespace {
                    output.push(' ');
                    previous_was_whitespace = true;
                }
            }
            _ if in_tag => {}
            _ if ch.is_whitespace() => {
                if !previous_was_whitespace {
                    output.push(' ');
                    previous_was_whitespace = true;
                }
            }
            _ => {
                output.push(ch);
                previous_was_whitespace = false;
            }
        }
    }

    output.trim().to_string()
}

impl fmt::Debug for TavilyRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TavilyRef")
            .field("api_token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}
