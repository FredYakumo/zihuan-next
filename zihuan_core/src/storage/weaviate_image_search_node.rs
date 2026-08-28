use std::cmp::Ordering;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::WeaviateClient;
use crate::weaviate::WeaviateRef;

pub const DEFAULT_MAX_DISTANCE: f64 = 0.55;

pub fn search_images(
    weaviate_ref: &Arc<WeaviateRef>,
    embedding_model: &dyn EmbeddingBase,
    query: &str,
    limit: usize,
    max_distance: Option<f64>,
    target_vector: Option<&str>,
) -> Result<Vec<Value>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::ValidationError("query is required".to_string()));
    }
    if limit == 0 {
        return Err(Error::ValidationError("limit must be greater than 0".to_string()));
    }
    let query_vector = embedding_model.inference(query)?;
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
        Some(target_vector.unwrap_or("description_vector")),
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
    Ok(items
        .into_iter()
        .filter_map(|item| normalized_image_item(&item, max_distance))
        .collect())
}

fn normalized_image_item(item: &Value, max_distance: Option<f64>) -> Option<Value> {
    let rustfs_path = string_field(item, "rustfs_path")?;
    let distance = distance_field(item);
    if max_distance.is_some_and(|threshold| distance.is_some_and(|value| value > threshold)) {
        return None;
    }
    let mut object = Map::new();
    for key in ["media_id", "original_source", "name", "description", "mime_type"] {
        object.insert(
            key.to_string(),
            string_field(item, key).map(Value::String).unwrap_or(Value::Null),
        );
    }
    object.insert("rustfs_path".to_string(), Value::String(rustfs_path));
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
    distance_field(left)
        .unwrap_or(f64::INFINITY)
        .partial_cmp(&distance_field(right).unwrap_or(f64::INFINITY))
        .unwrap_or(Ordering::Equal)
}
