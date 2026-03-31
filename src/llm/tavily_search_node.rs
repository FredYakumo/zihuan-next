use crate::error::{Error, Result};
use crate::node::data_value::TavilyRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
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

#[cfg(test)]
mod tests {
    use super::{TavilyRef, TavilySearchNode};
    use crate::error::Result;
    use crate::llm::tavily_provider_node::TavilyProviderNode;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    fn collect_strings(value: &DataValue) -> Vec<String> {
        match value {
            DataValue::Vec(inner, items) => {
                assert_eq!(inner.as_ref(), &DataType::String);
                items
                    .iter()
                    .map(|item| match item {
                        DataValue::String(value) => value.clone(),
                        other => panic!("unexpected item: {:?}", other),
                    })
                    .collect()
            }
            other => panic!("unexpected results output: {:?}", other),
        }
    }

    fn start_server(status_line: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
        let address = listener.local_addr().expect("local addr should exist");
        let status_line = status_line.to_string();
        let body = body.to_string();

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("one request expected");
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("response should be written");
        });

        format!("http://{}", address)
    }

    #[test]
    fn formats_multiple_results_in_order() {
        let response = super::TavilySearchResponse {
            results: vec![
                super::TavilySearchItem {
                    title: "First".to_string(),
                    url: "https://example.com/1".to_string(),
                    content: "Alpha".to_string(),
                },
                super::TavilySearchItem {
                    title: "Second".to_string(),
                    url: "https://example.com/2".to_string(),
                    content: "Beta".to_string(),
                },
            ],
        };

        let results = TavilySearchNode::parse_results(response);
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            "标题: First\n链接: https://example.com/1\n内容: Alpha"
        );
        assert_eq!(
            results[1],
            "标题: Second\n链接: https://example.com/2\n内容: Beta"
        );
    }

    #[test]
    fn empty_results_are_allowed() {
        let results = TavilySearchNode::parse_results(super::TavilySearchResponse {
            results: Vec::new(),
        });

        assert!(results.is_empty());
    }

    #[test]
    fn rejects_blank_query() {
        let mut node = TavilySearchNode::new("search", "Search");
        let err = node
            .execute(HashMap::from([
                (
                    "tavily_ref".to_string(),
                    DataValue::TavilyRef(Arc::new(TavilyRef::new(
                        "secret",
                        Duration::from_secs(5),
                    ))),
                ),
                ("query".to_string(), DataValue::String("   ".to_string())),
                ("search_count".to_string(), DataValue::Integer(3)),
            ]))
            .expect_err("blank query should be rejected");

        assert!(err.to_string().contains("query"));
    }

    #[test]
    fn rejects_non_positive_search_count() {
        let mut node = TavilySearchNode::new("search", "Search");
        let err = node
            .execute(HashMap::from([
                (
                    "tavily_ref".to_string(),
                    DataValue::TavilyRef(Arc::new(TavilyRef::new(
                        "secret",
                        Duration::from_secs(5),
                    ))),
                ),
                ("query".to_string(), DataValue::String("rust".to_string())),
                ("search_count".to_string(), DataValue::Integer(0)),
            ]))
            .expect_err("non-positive search_count should be rejected");

        assert!(err.to_string().contains("search_count"));
    }

    #[test]
    fn handles_http_success_response() -> Result<()> {
        let endpoint = start_server(
            "HTTP/1.1 200 OK",
            r#"{"results":[{"title":"Rust","url":"https://example.com/rust","content":"Rust language"}]}"#,
        );
        let tavily_ref = TavilyRef::new("secret-token", Duration::from_secs(5));

        let results = TavilySearchNode::execute_with_endpoint(&tavily_ref, "rust", 1, &endpoint)?;
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0],
            "标题: Rust\n链接: https://example.com/rust\n内容: Rust language"
        );

        Ok(())
    }

    #[test]
    fn returns_error_on_http_failure() {
        let endpoint = start_server(
            "HTTP/1.1 401 Unauthorized",
            r#"{"error":"invalid api key"}"#,
        );
        let tavily_ref = TavilyRef::new("secret-token", Duration::from_secs(5));

        let err = TavilySearchNode::execute_with_endpoint(&tavily_ref, "rust", 1, &endpoint)
            .expect_err("http failure should return error");

        assert!(err.to_string().contains("401"));
    }

    #[test]
    fn returns_error_on_missing_fields() {
        let endpoint = start_server("HTTP/1.1 200 OK", r#"{"results":[{"title":"Rust"}]}"#);
        let tavily_ref = TavilyRef::new("secret-token", Duration::from_secs(5));

        let err = TavilySearchNode::execute_with_endpoint(&tavily_ref, "rust", 1, &endpoint)
            .expect_err("missing fields should fail");

        assert!(err.to_string().contains("missing field"));
    }

    #[test]
    fn provider_output_can_feed_search_execution() -> Result<()> {
        let endpoint = start_server(
            "HTTP/1.1 200 OK",
            r#"{"results":[{"title":"Rust","url":"https://example.com/rust","content":"Rust language"}]}"#,
        );
        let mut provider = TavilyProviderNode::new("provider", "Provider");
        let search = TavilySearchNode::new("search", "Search");

        let provider_outputs = provider.execute(HashMap::from([(
            "api_token".to_string(),
            DataValue::Password("secret-token".to_string()),
        )]))?;

        let tavily_ref = provider_outputs
            .get("tavily_ref")
            .cloned()
            .expect("provider should output tavily_ref");

        let results = match &tavily_ref {
            DataValue::TavilyRef(tavily_ref) => {
                TavilySearchNode::execute_with_endpoint(tavily_ref, "rust", 1, &endpoint)?
            }
            other => panic!("unexpected provider output: {:?}", other),
        };

        let outputs = HashMap::from([(
            "results".to_string(),
            DataValue::Vec(
                Box::new(DataType::String),
                results.into_iter().map(DataValue::String).collect(),
            ),
        )]);
        search.validate_outputs(&outputs)?;
        assert_eq!(
            collect_strings(outputs.get("results").expect("results output should exist")),
            vec!["标题: Rust\n链接: https://example.com/rust\n内容: Rust language".to_string()]
        );

        Ok(())
    }
}
