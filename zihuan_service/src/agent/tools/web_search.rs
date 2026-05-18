use std::sync::Arc;

use log::warn;
use serde_json::Value;

use zihuan_agent::brain::BrainTool;
use zihuan_core::error::Result;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::rag::TavilyRef;

use super::common::{StaticFunctionToolSpec, ToolNotificationTarget};

const LOG_PREFIX: &str = "[QqChatAgent]";

pub(crate) struct WebSearchBrainTool {
    tavily_ref: Arc<TavilyRef>,
    notification_target: ToolNotificationTarget,
}

impl WebSearchBrainTool {
    pub(crate) fn new(
        tavily_ref: Arc<TavilyRef>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            tavily_ref,
            notification_target,
        }
    }

    fn extract_url_with_fallback(&self, url: &str) -> Result<Vec<String>> {
        match self.tavily_ref.extract_url(url) {
            Ok(items) => Ok(items),
            Err(e) => {
                warn!("{LOG_PREFIX} Tavily extract failed for url='{url}': {e}; trying direct web request");
                self.tavily_ref.fetch_url_direct(url)
            }
        }
    }

    fn search_with_fallback(&self, query: &str, search_count: i64) -> Result<Vec<String>> {
        match self.tavily_ref.search(query, search_count) {
            Ok(items) => Ok(items),
            Err(e) => {
                let trimmed = query.trim();
                if reqwest::Url::parse(trimmed).is_err() {
                    return Err(e);
                }

                warn!("{LOG_PREFIX} Tavily search failed for url-like query='{trimmed}': {e}; trying direct web request");
                self.tavily_ref.fetch_url_direct(trimmed)
            }
        }
    }
}

impl BrainTool for WebSearchBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "web_search",
            description:
                "使用 Tavily 在互联网上搜索信息，或对单个 URL 精确抽取页面内容，返回标题、链接和内容摘要",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词或问题；需要联网搜索多个结果时填写" },
                    "url": { "type": "string", "description": "要单独读取的网页 URL；用户明确给出单个 URL 并要求查看内容时填写，此时优先使用 Tavily Extract 精确抽取页面内容" },
                    "search_count": { "type": "integer", "description": "搜索结果数量，通常为 3，最大 10" }
                },
                "required": []
            }),
        })
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.notification_target.notify_progress(call_content);

        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let url = arguments
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let search_count = arguments
            .get("search_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(3);

        if url.is_empty() && query.trim().is_empty() {
            return serde_json::json!({"results": []}).to_string();
        }

        match if !url.is_empty() {
            self.extract_url_with_fallback(&url)
        } else {
            self.search_with_fallback(&query, search_count)
        } {
            Ok(items) => serde_json::json!({ "results": items }).to_string(),
            Err(e) => {
                warn!("{LOG_PREFIX} web_search failed: {e}");
                serde_json::json!({"results": [], "error": e.to_string()}).to_string()
            }
        }
    }
}
