use serde::Deserialize;

pub trait WebSearchEngine: Send + Sync {
    fn search(&self, query: &str, search_count: i64) -> crate::error::Result<Vec<String>>;
    fn extract_url(&self, url: &str) -> crate::error::Result<Vec<String>>;
    fn fetch_url_direct(&self, url: &str) -> crate::error::Result<Vec<String>>;
    fn search_images(
        &self,
        query: &str,
        max_results: i64,
    ) -> crate::error::Result<Vec<WebSearchImage>>;
}

#[derive(Debug, Deserialize)]
pub struct WebSearchImage {
    pub url: String,
    pub description: Option<String>,
}
