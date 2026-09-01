use std::sync::Arc;

use log::warn;
use serde_json::Value;

use crate::agent::tools::Tool;
use crate::error::Result;
use crate::model_inference::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use crate::rag::WebSearchEngine;

const LOG_PREFIX: &str = "[WebSearchTool]";

pub struct WebSearchTool {
    web_search_engine: std::result::Result<Arc<dyn WebSearchEngine>, String>,
}

impl WebSearchTool {
    pub fn new(web_search_engine: Arc<dyn WebSearchEngine>) -> Self {
        Self { web_search_engine: Ok(web_search_engine) }
    }

    pub fn unavailable(error: impl Into<String>) -> Self {
        Self { web_search_engine: Err(error.into()) }
    }

    fn extract_url_with_fallback(
        &self,
        engine: &dyn WebSearchEngine,
        url: &str,
    ) -> Result<Vec<String>> {
        match engine.extract_url(url) {
            Ok(items) => Ok(items),
            Err(error) => {
                warn!("{LOG_PREFIX} extract failed for url='{url}': {error}; trying direct web request");
                engine.fetch_url_direct(url)
            }
        }
    }

    fn search_with_fallback(
        &self,
        engine: &dyn WebSearchEngine,
        query: &str,
        search_count: i64,
    ) -> Result<Vec<String>> {
        match engine.search(query, search_count) {
            Ok(items) => Ok(items),
            Err(error) => {
                let trimmed = query.trim();
                if reqwest::Url::parse(trimmed).is_err() {
                    return Err(error);
                }

                warn!("{LOG_PREFIX} search failed for url-like query='{trimmed}': {error}; trying direct web request");
                engine.fetch_url_direct(trimmed)
            }
        }
    }
}

impl Tool for WebSearchTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "web_search",
            description:
                "在互联网上检索信息，或读取单个 URL 页面内容，返回可用于回答的问题相关结果与摘要",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词或问题；需要联网搜索多个结果时填写" },
                    "url": { "type": "string", "description": "要单独读取的网页 URL；用户明确给出单个 URL 并要求查看页面内容时填写" },
                    "search_count": { "type": "integer", "description": "搜索结果数量，通常为 3，最大 10" }
                },
                "required": []
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let query = arguments.get("query").and_then(Value::as_str).unwrap_or("");
        let url = arguments.get("url").and_then(Value::as_str).unwrap_or("").trim();
        let search_count = arguments.get("search_count").and_then(Value::as_i64).unwrap_or(3);

        if url.is_empty() && query.trim().is_empty() {
            return serde_json::json!({"results": []}).to_string();
        }

        let engine = match &self.web_search_engine {
            Ok(engine) => engine,
            Err(error) => {
                warn!("{LOG_PREFIX} unavailable: {error}");
                return serde_json::json!({"results": [], "error": error}).to_string();
            }
        };
        let result = if !url.is_empty() {
            self.extract_url_with_fallback(engine.as_ref(), url)
        } else {
            self.search_with_fallback(engine.as_ref(), query, search_count)
        };
        match result {
            Ok(items) => serde_json::json!({ "results": items }).to_string(),
            Err(error) => {
                warn!("{LOG_PREFIX} failed: {error}");
                serde_json::json!({"results": [], "error": error.to_string()}).to_string()
            }
        }
    }
}
