use std::collections::HashMap;

use general_wheel_cpp::top_k_similar;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct TopKSimilarityNode {
    id: String,
    name: String,
}

impl TopKSimilarityNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for TopKSimilarityNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("对 Vec<Vector> 与查询向量执行 top-k 相似度检索")
    }

    node_input![
        port! { name = "vectors", ty = Vec(Vector), desc = "候选向量列表" },
        port! { name = "query", ty = Vector, desc = "查询向量" },
        port! { name = "top_k", ty = Integer, desc = "返回最相似的前 K 个结果" },
    ];

    node_output![
        port! { name = "indices", ty = Vec(Integer), desc = "命中向量索引列表，按相似度降序排列" },
        port! { name = "scores", ty = Vec(Float), desc = "对应命中结果的相似度分数" },
        port! { name = "vectors", ty = Vec(Vector), desc = "命中的向量列表，按相似度降序排列" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let candidates = parse_vector_list(inputs.get("vectors"))?;
        let query = parse_vector(inputs.get("query"))?;
        let top_k = match inputs.get("top_k") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) => {
                return Err(Error::ValidationError(
                    "top_k must be greater than 0".to_string(),
                ));
            }
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: top_k".to_string(),
                ));
            }
        };

        let top_results = top_k_similar(&candidates, &query, top_k)
            .map_err(|error| Error::StringError(error.to_string()))?;

        let indices = top_results
            .iter()
            .map(|(index, _)| DataValue::Integer(*index as i64))
            .collect::<Vec<_>>();
        let scores = top_results
            .iter()
            .map(|(_, score)| DataValue::Float(*score as f64))
            .collect::<Vec<_>>();
        let matched_vectors = top_results
            .into_iter()
            .map(|(index, _)| DataValue::Vector(candidates[index].clone()))
            .collect::<Vec<_>>();

        let outputs = HashMap::from([
            (
                "indices".to_string(),
                DataValue::Vec(Box::new(DataType::Integer), indices),
            ),
            (
                "scores".to_string(),
                DataValue::Vec(Box::new(DataType::Float), scores),
            ),
            (
                "vectors".to_string(),
                DataValue::Vec(Box::new(DataType::Vector), matched_vectors),
            ),
        ]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn parse_vector(value: Option<&DataValue>) -> Result<Vec<f32>> {
    match value {
        Some(DataValue::Vector(values)) if !values.is_empty() => Ok(values.clone()),
        Some(DataValue::Vector(_)) => Err(Error::ValidationError(
            "query vector must not be empty".to_string(),
        )),
        _ => Err(Error::ValidationError(
            "query input must be Vector".to_string(),
        )),
    }
}

fn parse_vector_list(value: Option<&DataValue>) -> Result<Vec<Vec<f32>>> {
    let values = match value {
        Some(DataValue::Vec(_, values)) => values,
        _ => {
            return Err(Error::ValidationError(
                "vectors input must be Vec<Vector>".to_string(),
            ));
        }
    };

    let vectors = values
        .iter()
        .map(|value| match value {
            DataValue::Vector(vector) if !vector.is_empty() => Ok(vector.clone()),
            DataValue::Vector(_) => Err(Error::ValidationError(
                "vectors input must not contain empty vectors".to_string(),
            )),
            other => Err(Error::ValidationError(format!(
                "vectors input must contain Vector values, got {}",
                other.data_type()
            ))),
        })
        .collect::<Result<Vec<_>>>()?;

    if vectors.is_empty() {
        return Err(Error::ValidationError(
            "vectors input must not be empty".to_string(),
        ));
    }

    Ok(vectors)
}
