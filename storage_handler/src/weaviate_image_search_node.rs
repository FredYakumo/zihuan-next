use std::cmp::Ordering;
use std::collections::HashMap;

use serde_json::{Map, Value};
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct WeaviateImageSearchNode {
    id: String,
    name: String,
}

const DEFAULT_MAX_DISTANCE: f64 = 0.55;

impl WeaviateImageSearchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for WeaviateImageSearchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用本地 Weaviate 图片库做语义检索，输出标准化图片结果 JSON")
    }

    node_input![
        port! { name = "weaviate_ref", ty = WeaviateRef, desc = "Weaviate 图片 collection 引用" },
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "用于查询向量化的 Embedding 模型" },
        port! { name = "query", ty = String, desc = "图片语义查询文本" },
        port! { name = "limit", ty = Integer, desc = "返回结果数量，必须大于 0" },
        port! { name = "max_distance", ty = Float, desc = "可选：最大允许距离，超过则过滤", optional },
    ];

    node_output![
        port! { name = "images", ty = Json, desc = "标准化图片结果数组 JSON" },
        port! { name = "has_results", ty = Boolean, desc = "是否命中至少一张图片" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let weaviate_ref = match inputs.get("weaviate_ref") {
            Some(DataValue::WeaviateRef(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: weaviate_ref".to_string(),
                ))
            }
        };
        let embedding_model = match inputs.get("embedding_model") {
            Some(DataValue::EmbeddingModel(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: embedding_model".to_string(),
                ))
            }
        };
        let query = match inputs.get("query") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: query".to_string(),
                ))
            }
        };
        let limit = match inputs.get("limit") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) => {
                return Err(Error::ValidationError(
                    "limit must be greater than 0".to_string(),
                ))
            }
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: limit".to_string(),
                ))
            }
        };
        let max_distance = match inputs.get("max_distance") {
            Some(DataValue::Float(value)) if *value >= 0.0 => Some(*value),
            Some(DataValue::Integer(value)) if *value >= 0 => Some(*value as f64),
            Some(DataValue::Float(_)) | Some(DataValue::Integer(_)) | None => {
                Some(DEFAULT_MAX_DISTANCE)
            }
            Some(_) => {
                return Err(Error::ValidationError(
                    "max_distance must be a non-negative number".to_string(),
                ))
            }
        };

        let query_vector = embedding_model.inference(&query)?;
        if query_vector.is_empty() {
            return Err(Error::ValidationError(
                "embedding_model returned an empty query vector".to_string(),
            ));
        }

        let property_names = vec![
            "media_id".to_string(),
            "original_source".to_string(),
            "rustfs_path".to_string(),
            "name".to_string(),
            "description".to_string(),
            "mime_type".to_string(),
            "source".to_string(),
        ];
        let response = weaviate_ref.query_near_vector(
            &weaviate_ref.class_name,
            &query_vector,
            limit,
            &property_names,
            true,
            false,
        )?;

        let mut items = response
            .get("data")
            .and_then(|value| value.get("Get"))
            .and_then(|value| value.get(&weaviate_ref.class_name))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        items.sort_by(semantic_result_order);

        let images = items
            .into_iter()
            .filter_map(|item| normalized_image_item(&item, max_distance))
            .collect::<Vec<_>>();

        let outputs = HashMap::from([
            (
                "images".to_string(),
                DataValue::Json(Value::Array(images.clone())),
            ),
            (
                "has_results".to_string(),
                DataValue::Boolean(!images.is_empty()),
            ),
        ]);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn normalized_image_item(item: &Value, max_distance: Option<f64>) -> Option<Value> {
    let rustfs_path = string_field(item, "rustfs_path")?;
    let distance = distance_field(item);
    if max_distance.is_some_and(|threshold| distance.is_some_and(|value| value > threshold)) {
        return None;
    }

    let mut object = Map::new();
    object.insert(
        "media_id".to_string(),
        string_field(item, "media_id")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "original_source".to_string(),
        string_field(item, "original_source")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "rustfs_path".to_string(),
        Value::String(rustfs_path),
    );
    object.insert(
        "name".to_string(),
        string_field(item, "name")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "description".to_string(),
        string_field(item, "description")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "mime_type".to_string(),
        string_field(item, "mime_type")
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    object.insert(
        "source".to_string(),
        string_field(item, "source")
            .map(Value::String)
            .unwrap_or_else(|| Value::String("weaviate".to_string())),
    );
    if let Some(distance) = distance {
        object.insert("distance".to_string(), serde_json::json!(distance));
    }

    Some(Value::Object(object))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn distance_field(value: &Value) -> Option<f64> {
    value
        .get("_additional")
        .and_then(|extra| extra.get("distance"))
        .and_then(Value::as_f64)
}

fn semantic_result_order(left: &Value, right: &Value) -> Ordering {
    let left_distance = distance_field(left).unwrap_or(f64::INFINITY);
    let right_distance = distance_field(right).unwrap_or(f64::INFINITY);
    left_distance
        .partial_cmp(&right_distance)
        .unwrap_or(Ordering::Equal)
}
