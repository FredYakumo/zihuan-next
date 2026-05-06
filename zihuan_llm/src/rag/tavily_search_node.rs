use std::collections::HashMap;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

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
            return Err(Error::ValidationError(
                "query must not be blank".to_string(),
            ));
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

        let results = tavily_ref.search(&query, search_count)?;

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

