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

#[cfg(test)]
mod tests {
    use super::JsonToQQMessageVecNode;
    use zihuan_bot_types::message::Message;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn outputs_vec_of_qq_message_batches() {
        let mut node = JsonToQQMessageVecNode::new("parser", "Parser");
        let outputs = node
            .execute(HashMap::from([(
                "json".to_string(),
                DataValue::Json(serde_json::json!([
                    [{"message_type":"plain_text","content":"你好"}],
                    [{"message_type":"at","target":"42"},{"message_type":"plain_text","content":"第二条"}]
                ])),
            )]))
            .expect("json parser node should execute");

        match outputs.get("messages") {
            Some(DataValue::Vec(inner, batches)) => {
                assert_eq!(**inner, DataType::Vec(Box::new(DataType::QQMessage)));
                assert_eq!(batches.len(), 2);

                match &batches[0] {
                    DataValue::Vec(batch_inner, items) => {
                        assert_eq!(**batch_inner, DataType::QQMessage);
                        assert_eq!(items.len(), 1);
                    }
                    other => panic!("unexpected first batch output: {:?}", other),
                }
            }
            other => panic!("unexpected messages output: {:?}", other),
        }
    }

    #[test]
    fn preserves_combine_text_inside_batch() {
        let mut node = JsonToQQMessageVecNode::new("parser", "Parser");
        let outputs = node
            .execute(HashMap::from([(
                "json".to_string(),
                DataValue::Json(serde_json::json!([
                    [{"message_type":"combine_text","content_list":[
                        {"message_type":"at","target":"42"},
                        {"message_type":"plain_text","content":"你好"}
                    ]}]
                ])),
            )]))
            .expect("json parser node should execute");

        match outputs.get("messages") {
            Some(DataValue::Vec(_, batches)) => match &batches[0] {
                DataValue::Vec(_, items) => {
                    assert!(matches!(items.as_slice(), [
                        DataValue::QQMessage(Message::At(_)),
                        DataValue::QQMessage(Message::PlainText(_))
                    ]));
                }
                other => panic!("unexpected first batch output: {:?}", other),
            },
            other => panic!("unexpected messages output: {:?}", other),
        }
    }

    #[test]
    fn rejects_non_json_input() {
        let mut node = JsonToQQMessageVecNode::new("parser", "Parser");
        let error = node
            .execute(HashMap::from([(
                "json".to_string(),
                DataValue::String("not json".to_string()),
            )]))
            .expect_err("non-json typed input should be rejected");

        assert!(!error.to_string().is_empty());
    }

    #[test]
    fn rejects_legacy_one_dimensional_format() {
        let mut node = JsonToQQMessageVecNode::new("parser", "Parser");
        let outputs = node
            .execute(HashMap::from([(
                "json".to_string(),
                DataValue::Json(serde_json::json!([
                    {"message_type":"plain_text","content":"你好"}
                ])),
            )]))
            .expect("legacy one-dimensional format should route to failed output");

        assert!(
            outputs.contains_key("failed"),
            "expected failed output key, got: {:?}",
            outputs.keys().collect::<Vec<_>>()
        );
        assert!(!outputs.contains_key("messages"));
    }
}
