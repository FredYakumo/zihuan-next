use std::collections::HashMap;

use log::warn;
use reqwest::Url;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct TavilyWebSearchNode {
    id: String,
    name: String,
}

impl TavilyWebSearchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for TavilyWebSearchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 Tavily 搜索网页，或对单个 URL 抽取正文内容")
    }

    node_input![
        port! { name = "tavily_ref", ty = TavilyRef, desc = "Tavily 搜索引用" },
        port! { name = "query", ty = String, desc = "搜索关键词或问题", optional },
        port! { name = "url", ty = String, desc = "要单独抽取的网页 URL", optional },
        port! { name = "search_count", ty = Integer, desc = "搜索结果数量，默认 3", optional },
    ];

    node_output![
        port! { name = "results", ty = Vec(String), desc = "搜索或抽取得到的文本结果列表" },
    ];

    fn execute(
        &mut self,
        inputs: zihuan_graph_engine::NodeInputFlow,
    ) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let tavily_ref = inputs
            .get("tavily_ref")
            .and_then(|value| match value {
                DataValue::TavilyRef(tavily_ref) => Some(tavily_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("tavily_ref is required".to_string()))?;

        let query = inputs
            .get("query")
            .and_then(|value| match value {
                DataValue::String(text) => Some(text.trim().to_string()),
                _ => None,
            })
            .unwrap_or_default();

        let url = inputs
            .get("url")
            .and_then(|value| match value {
                DataValue::String(text) => Some(text.trim().to_string()),
                _ => None,
            })
            .unwrap_or_default();

        let search_count = inputs
            .get("search_count")
            .and_then(|value| match value {
                DataValue::Integer(count) => Some(*count),
                _ => None,
            })
            .unwrap_or(3);

        if url.is_empty() && query.is_empty() {
            return Err(Error::ValidationError(
                "query 和 url 不能同时为空".to_string(),
            ));
        }

        let results = if !url.is_empty() {
            match tavily_ref.extract_url(&url) {
                Ok(items) => items,
                Err(error) => {
                    warn!(
                        "[TavilyWebSearchNode:{}] extract failed for url='{}': {}; trying direct web request",
                        self.id, url, error
                    );
                    tavily_ref.fetch_url_direct(&url)?
                }
            }
        } else {
            match tavily_ref.search(&query, search_count) {
                Ok(items) => items,
                Err(error) => {
                    if Url::parse(&query).is_err() {
                        return Err(error);
                    }

                    warn!(
                        "[TavilyWebSearchNode:{}] search failed for url-like query='{}': {}; trying direct web request",
                        self.id, query, error
                    );
                    tavily_ref.fetch_url_direct(&query)?
                }
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "results".to_string(),
            DataValue::Vec(
                Box::new(DataType::String),
                results.into_iter().map(DataValue::String).collect(),
            ),
        );
        let outputs = zihuan_graph_engine::NodeOutputFlow::from(outputs);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

