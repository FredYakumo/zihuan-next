use zihuan_core::error::{Error, Result};
use zihuan_node::data_value::TavilyRef;
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const TAVILY_SEARCH_ENDPOINT: &str = "https://api.tavily.com/search";

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

pub struct TavilySearchNode {
    id: String,
    name: String,
}

impl TavilySearchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn build_request_body(query: &str, search_count: i64) -> Value {
        json!({
            "query": query,
            "max_results": search_count,
            "search_depth": "advanced",
            "include_answer": false,
            "include_images": false,
            "include_raw_content": false,
        })
    }

    fn format_result(item: TavilySearchItem) -> String {
        format!(
            "标题: {}\n链接: {}\n内容: {}",
            item.title, item.url, item.content
        )
    }

    fn parse_results(response: TavilySearchResponse) -> Vec<String> {
        response
            .results
            .into_iter()
            .map(Self::format_result)
            .collect()
    }

    fn execute_with_endpoint(
        tavily_ref: &TavilyRef,
        query: &str,
        search_count: i64,
        endpoint: &str,
    ) -> Result<Vec<String>> {
        let client = Client::builder().timeout(tavily_ref.timeout).build()?;
        let response = client
            .post(endpoint)
            .bearer_auth(&tavily_ref.api_token)
            .json(&Self::build_request_body(query, search_count))
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(Error::StringError(format!(
                "Tavily search request failed with status {}: {}",
                status, body
            )));
        }

        let body = response.text()?;
        let parsed: TavilySearchResponse = serde_json::from_str(&body).map_err(|err| {
            Error::StringError(format!("Failed to parse Tavily search response: {err}"))
        })?;
        Ok(Self::parse_results(parsed))
    }
}

impl Node for TavilySearchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 TavilyRef 执行搜索，输出包含标题、链接和内容的 Vec<String>")
    }

    node_input![
        port! { name = "tavily_ref", ty = DataType::TavilyRef, desc = "Tavily 搜索引用" },
        port! { name = "query", ty = String, desc = "搜索关键词或问题" },
        port! { name = "search_count", ty = Integer, desc = "返回结果数量，必须大于 0" },
    ];

    node_output![
        port! { name = "results", ty = Vec(String), desc = "搜索结果列表，每项包含标题、链接、内容" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let tavily_ref = match inputs.get("tavily_ref") {
            Some(DataValue::TavilyRef(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: tavily_ref".to_string(),
                ))
            }
        };

        let query = match inputs.get("query") {
            Some(DataValue::String(value)) => value.trim().to_string(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: query".to_string(),
                ))
            }
        };

        if query.is_empty() {
            return Err(Error::ValidationError("query must not be blank".to_string()));
        }

        let search_count = match inputs.get("search_count") {
            Some(DataValue::Integer(value)) => *value,
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: search_count".to_string(),
                ))
            }
        };

        if search_count <= 0 {
            return Err(Error::ValidationError(
                "search_count must be greater than 0".to_string(),
            ));
        }

        let results =
            Self::execute_with_endpoint(tavily_ref.as_ref(), &query, search_count, TAVILY_SEARCH_ENDPOINT)?;

        let outputs = HashMap::from([(
            "results".to_string(),
            DataValue::Vec(
                Box::new(DataType::String),
                results.into_iter().map(DataValue::String).collect(),
            ),
        )]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

