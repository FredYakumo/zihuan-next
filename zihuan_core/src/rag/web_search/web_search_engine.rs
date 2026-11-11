use crate::connection::Connection;
use crate::error::Result;
use serde::Deserialize;

/// Search capabilities shared by plugin-backed and other search providers.
pub trait WebSearchEngine: Connection {
    fn search(&self, query: &str, search_count: i64) -> Result<Vec<String>>;
    fn extract_url(&self, url: &str) -> Result<Vec<String>>;
    fn fetch_url_direct(&self, url: &str) -> Result<Vec<String>>;
    fn search_images(&self, query: &str, max_results: i64) -> Result<Vec<WebSearchImage>>;
}

#[derive(Debug, Deserialize)]
pub struct WebSearchImage {
    pub url: String,
    pub description: Option<String>,
}
