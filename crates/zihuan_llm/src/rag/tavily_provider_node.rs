use zihuan_core::error::{Error, Result};
use zihuan_node::data_value::TavilyRef;
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub struct TavilyProviderNode {
    id: String,
    name: String,
}

impl TavilyProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for TavilyProviderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("配置 Tavily 搜索 API 令牌，输出 TavilyRef 引用供下游搜索节点使用")
    }

    node_input![
        port! { name = "api_token", ty = Password, desc = "Tavily API Token" },
        port! { name = "timeout_secs", ty = Integer, desc = "可选：请求超时秒数，默认 30 秒", optional },
    ];

    node_output![
        port! { name = "tavily_ref", ty = DataType::TavilyRef, desc = "Tavily 搜索引用" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let api_token = match inputs.get("api_token") {
            Some(DataValue::Password(value)) => value.trim().to_string(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: api_token".to_string(),
                ))
            }
        };

        if api_token.is_empty() {
            return Err(Error::ValidationError(
                "api_token must not be empty".to_string(),
            ));
        }

        let timeout_secs = match inputs.get("timeout_secs") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as u64,
            Some(DataValue::Integer(_)) => {
                return Err(Error::ValidationError(
                    "timeout_secs must be greater than 0".to_string(),
                ))
            }
            Some(_) => {
                return Err(Error::ValidationError(
                    "timeout_secs must be an integer".to_string(),
                ))
            }
            None => 30,
        };

        let tavily_ref = Arc::new(TavilyRef::new(
            api_token,
            Duration::from_secs(timeout_secs),
        ));

        let outputs = HashMap::from([(
            "tavily_ref".to_string(),
            DataValue::TavilyRef(tavily_ref),
        )]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

