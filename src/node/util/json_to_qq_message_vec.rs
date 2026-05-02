use crate::error::{Error, Result};
use zihuan_llm::natural_language_reply::json_value_to_qq_message_vec as parse_json_to_qq_messages;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use std::collections::HashMap;

pub struct JsonToQQMessageVecNode {
    id: String,
    name: String,
}

impl JsonToQQMessageVecNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for JsonToQQMessageVecNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 LLM 输出的 QQ 消息 JSON 二维数组转换为 Vec<Vec<QQMessage>>")
    }

    node_input![port! { name = "json", ty = Json, desc = "LLM 输出的 QQ 消息 JSON 二维数组" },];

    node_output![
        port! { name = "messages", ty = Vec(Vec(QQMessage)), desc = "解析得到的 Vec<Vec<QQMessage>>" },
        port! { name = "failed", ty = Json, desc = "转换失败时输出原始 JSON" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let json_value = match inputs.get("json") {
            Some(DataValue::Json(value)) => value,
            _ => return Err(Error::InvalidNodeInput("json is required".to_string())),
        };

        match parse_json_to_qq_messages(json_value) {
            Ok(messages) => {
                info!(
                    "[JsonToQQMessageVecNode] Conversion succeeded: {} message batch(es)",
                    messages.len()
                );
                let outputs = HashMap::from([(
                    "messages".to_string(),
                    DataValue::Vec(
                        Box::new(DataType::Vec(Box::new(DataType::QQMessage))),
                        messages
                            .into_iter()
                            .map(|batch| {
                                DataValue::Vec(
                                    Box::new(DataType::QQMessage),
                                    batch.into_iter().map(DataValue::QQMessage).collect(),
                                )
                            })
                            .collect(),
                    ),
                )]);
                self.validate_outputs(&outputs)?;
                Ok(outputs)
            }
            Err(e) => {
                warn!(
                    "[JsonToQQMessageVecNode] Conversion failed: {} — routing to failed output",
                    e
                );
                let outputs = HashMap::from([(
                    "failed".to_string(),
                    DataValue::Json(json_value.clone()),
                )]);
                self.validate_outputs(&outputs)?;
                Ok(outputs)
            }
        }
    }
}

