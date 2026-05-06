use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct BatchTextEmbeddingNode {
    id: String,
    name: String,
}

impl BatchTextEmbeddingNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BatchTextEmbeddingNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 EmbeddingModel 批量将文本编码为向量，输出 Vec<Vector>")
    }

    node_input![
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "embedding 模型引用" },
        port! { name = "texts", ty = Vec(String), desc = "待批量编码的文本列表" },
    ];

    node_output![
        port! { name = "embeddings", ty = Vec(Vector), desc = "批量文本向量" },
        port! { name = "count", ty = Integer, desc = "输出向量数量" },
        port! { name = "dimension", ty = Integer, desc = "向量维度，若为空则为 0" },
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
                ))
            }
        };

        let texts = parse_string_list(inputs.get("texts"))?;
        let embeddings = embedding_model.batch_inference(&texts)?;
        let count = embeddings.len() as i64;
        let dimension = embeddings
            .first()
            .map(|item| item.len() as i64)
            .unwrap_or(0);
        let values = embeddings
            .into_iter()
            .map(DataValue::Vector)
            .collect::<Vec<_>>();

        let outputs = HashMap::from([
            (
                "embeddings".to_string(),
                DataValue::Vec(Box::new(DataType::Vector), values),
            ),
            ("count".to_string(), DataValue::Integer(count)),
            ("dimension".to_string(), DataValue::Integer(dimension)),
        ]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn parse_string_list(value: Option<&DataValue>) -> Result<Vec<String>> {
    let values = match value {
        Some(DataValue::Vec(_, values)) => values,
        _ => {
            return Err(Error::ValidationError(
                "texts input must be Vec<String>".to_string(),
            ))
        }
    };

    let texts = values
        .iter()
        .map(|value| match value {
            DataValue::String(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
            DataValue::String(_) => Err(Error::ValidationError(
                "texts input must not contain blank strings".to_string(),
            )),
            other => Err(Error::ValidationError(format!(
                "texts input must contain String values, got {}",
                other.data_type()
            ))),
        })
        .collect::<Result<Vec<_>>>()?;

    if texts.is_empty() {
        return Err(Error::ValidationError(
            "texts input must not be empty".to_string(),
        ));
    }

    Ok(texts)
}
