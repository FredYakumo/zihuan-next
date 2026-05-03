use std::collections::HashMap;

use general_wheel_cpp::cosine_similarity;
use zihuan_core::error::{Error, Result};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct VectorCosineSimilarityNode {
    id: String,
    name: String,
}

impl VectorCosineSimilarityNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for VectorCosineSimilarityNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 general-wheel-cpp 计算两个向量的余弦相似度")
    }

    node_input![
        port! { name = "left", ty = Vec(Float), desc = "左侧向量" },
        port! { name = "right", ty = Vec(Float), desc = "右侧向量" },
    ];

    node_output![
        port! { name = "similarity", ty = Float, desc = "余弦相似度，范围通常为 [-1, 1]" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let left = parse_float_vector(inputs.get("left"))?;
        let right = parse_float_vector(inputs.get("right"))?;
        let similarity = cosine_similarity(&left, &right)
            .map_err(|error| Error::StringError(error.to_string()))?;

        let outputs = HashMap::from([(
            "similarity".to_string(),
            DataValue::Float(similarity as f64),
        )]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn parse_float_vector(value: Option<&DataValue>) -> Result<Vec<f32>> {
    let values = match value {
        Some(DataValue::Vec(_, values)) => values,
        _ => {
            return Err(Error::ValidationError(
                "vector input must be Vec<Float>".to_string(),
            ))
        }
    };

    values
        .iter()
        .map(|value| match value {
            DataValue::Float(raw) => Ok(*raw as f32),
            DataValue::Integer(raw) => Ok(*raw as f32),
            other => Err(Error::ValidationError(format!(
                "vector input must contain Float values, got {}",
                other.data_type()
            ))),
        })
        .collect()
}
