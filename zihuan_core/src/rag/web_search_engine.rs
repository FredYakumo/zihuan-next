use serde::Deserialize;
use std::fmt;
use std::sync::Arc;

pub trait WebSearchEngine: Send + Sync {
    fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>>;
    fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>>;
    fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>>;
    fn search_images(&self, query: &str, max_results: i64) -> crate::error::Result<Vec<WebSearchImage>>;
}

#[derive(Clone)]
pub struct WebSearchEngineRef {
    pub engine: Arc<dyn WebSearchEngine>,
}

impl WebSearchEngineRef {
    pub fn new(engine: Arc<dyn WebSearchEngine>) -> Self {
        Self { engine }
    }

    pub fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>> {
        self.engine.search(query, search_count)
    }

    pub fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>> {
        self.engine.extract_url(url)
    }

    pub fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>> {
        self.engine.fetch_url_direct(url)
    }

    pub fn search_images(&self, query: &str, max_results: i64) -> crate::error::Result<Vec<WebSearchImage>> {
        self.engine.search_images(query, max_results)
    }
}

#[derive(Debug, Deserialize)]
pub struct WebSearchImage {
    pub url: String,
    pub description: Option<String>,
}

pub fn strip_html_tags(input: &str) -> String {
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

impl fmt::Debug for WebSearchEngineRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSearchEngineRef").finish()
    }
}
