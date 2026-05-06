use std::collections::HashMap;

use general_wheel_cpp::cosine_similarity;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

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
        port! { name = "left", ty = Vector, desc = "左侧向量" },
        port! { name = "right", ty = Vector, desc = "右侧向量" },
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
    match value {
        Some(DataValue::Vector(values)) => Ok(values.clone()),
        _ => Err(Error::ValidationError(
            "vector input must be Vector".to_string(),
        )),
    }
}

