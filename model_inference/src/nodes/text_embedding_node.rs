use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct TextEmbeddingNode {
    id: String,
    name: String,
}

impl TextEmbeddingNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for TextEmbeddingNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 EmbeddingModel 将文本编码为向量，输出 Vector")
    }

    node_input![
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "embedding 模型引用" },
        port! { name = "text", ty = String, desc = "待编码的文本" },
    ];

    node_output![
        port! { name = "embedding", ty = Vector, desc = "文本向量" },
        port! { name = "dimension", ty = Integer, desc = "向量维度" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let embedding_model = match inputs.get("embedding_model") {
            Some(DataValue::EmbeddingModel(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: embedding_model".to_string(),
                ));
            }
        };

        let text = match inputs.get("text") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: text".to_string(),
                ));
            }
        };

        let embedding = embedding_model.inference(&text)?;
        let dimension = embedding.len() as i64;
        let vector = DataValue::Vector(embedding);

        let outputs = HashMap::from([
            ("embedding".to_string(), vector),
            ("dimension".to_string(), DataValue::Integer(dimension)),
        ]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
