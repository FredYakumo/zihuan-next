use serde_json::Value;
use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_core::weaviate::{
    WeaviateCollectionConfig, WeaviateCollectionSchema, WeaviateEnsureCollectionResult,
    WeaviatePropertyConfig, WeaviateRef, WeaviateVectorConfigEntry,
};

pub fn collection_config_for_schema(
    schema: WeaviateCollectionSchema,
    class_name: String,
) -> WeaviateCollectionConfig {
    match schema {
        WeaviateCollectionSchema::MessageRecordSemantic => {
            message_vector_collection_config(class_name)
        }
        WeaviateCollectionSchema::ImageSemantic => image_vector_collection_config(class_name),
    }
}

pub fn validate_collection_schema(
    existing: &Value,
    expected: &WeaviateCollectionConfig,
) -> Result<()> {
    let existing_name = existing
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if existing_name != expected.class_name {
        return Err(Error::ValidationError(format!(
            "Weaviate collection name mismatch: expected '{}', got '{}'",
            expected.class_name, existing_name
        )));
    }
    if let Some(expected_vectorizer) = expected.vectorizer.as_deref() {
        let existing_vectorizer = existing
            .get("vectorizer")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if existing_vectorizer != expected_vectorizer {
            return Err(Error::ValidationError(format!(
                "Weaviate collection '{}' vectorizer mismatch: expected '{}', got '{}'",
                expected.class_name, expected_vectorizer, existing_vectorizer
            )));
        }
    }
    if let Some(expected_vector_config) = &expected.vector_config {
        let existing_vector_config = existing
            .get("vectorConfig")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (vector_name, expected_entry) in expected_vector_config {
            let Some(existing_entry) = existing_vector_config.get(vector_name) else {
                return Err(Error::ValidationError(format!(
                    "Weaviate collection '{}' missing vector config '{}'",
                    expected.class_name, vector_name
                )));
            };
            let existing_index_type = existing_entry
                .get("vectorIndexType")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if existing_index_type != expected_entry.vector_index_type {
                return Err(Error::ValidationError(format!(
                    "Weaviate collection '{}' vector '{}' index type mismatch: expected '{}', got '{}'",
                    expected.class_name, vector_name, expected_entry.vector_index_type, existing_index_type
                )));
            }
            let existing_vectorizer = existing_entry
                .get("vectorizer")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if existing_vectorizer != expected_entry.vectorizer {
                return Err(Error::ValidationError(format!(
                    "Weaviate collection '{}' vector '{}' vectorizer mismatch: expected '{}', got '{}'",
                    expected.class_name, vector_name, expected_entry.vectorizer, existing_vectorizer
                )));
            }
        }
    }
    let existing_properties = existing
        .get("properties")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for expected_property in &expected.properties {
        let Some(existing_property) = existing_properties.iter().find(|property| {
            property
                .get("name")
                .and_then(Value::as_str)
                .map(|name| name == expected_property.name)
                .unwrap_or(false)
        }) else {
            return Err(Error::ValidationError(format!(
                "Weaviate collection '{}' missing property '{}'",
                expected.class_name, expected_property.name
            )));
        };
        let existing_data_type = existing_property
            .get("dataType")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if existing_data_type != expected_property.data_type {
            return Err(Error::ValidationError(format!(
                "Weaviate collection '{}' property '{}' dataType mismatch: expected {:?}, got {:?}",
                expected.class_name,
                expected_property.name,
                expected_property.data_type,
                existing_data_type
            )));
        }
    }
    Ok(())
}

pub fn ensure_collection_schema(
    weaviate_ref: &WeaviateRef,
    schema: WeaviateCollectionSchema,
    create_missing: bool,
) -> Result<WeaviateEnsureCollectionResult> {
    let collection = collection_config_for_schema(schema, weaviate_ref.class_name.clone());
    match weaviate_ref.find_collection_schema(&collection.class_name)? {
        Some(existing) => {
            validate_collection_schema(&existing, &collection)?;
            Ok(WeaviateEnsureCollectionResult::Existing)
        }
        None if create_missing => {
            weaviate_ref.create_collection(&collection)?;
            Ok(WeaviateEnsureCollectionResult::Created)
        }
        None => Err(Error::ValidationError(format!(
            "Weaviate collection '{}' does not exist",
            collection.class_name
        ))),
    }
}

fn message_vector_collection_config(class_name: String) -> WeaviateCollectionConfig {
    WeaviateCollectionConfig {
        class_name,
        description: Some("QQ message vector storage".to_string()),
        properties: vec![
            text_property("message_id", "QQ platform message ID"),
            text_property("sender_id", "Sender ID"),
            text_property("sender_name", "Sender name"),
            date_property("send_time", "Message send time"),
            text_property("group_id", "Group ID, may be empty"),
            text_property("group_name", "Group name, may be empty"),
            text_property("content", "Aggregated message text"),
            text_property("at_target_list", "@ mention target list"),
            text_property("media_json", "Message media metadata JSON"),
        ],
        vectorizer: Some("none".to_string()),
        vector_config: None,
    }
}

fn text_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["text".to_string()],
        description: Some(description.to_string()),
    }
}

fn date_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["date".to_string()],
        description: Some(description.to_string()),
    }
}

fn image_vector_collection_config(class_name: String) -> WeaviateCollectionConfig {
    let mut vector_config = HashMap::new();
    vector_config.insert(
        "description_vector".to_string(),
        WeaviateVectorConfigEntry {
            vector_index_type: "hnsw".to_string(),
            vectorizer: "none".to_string(),
        },
    );
    vector_config.insert(
        "name_vector".to_string(),
        WeaviateVectorConfigEntry {
            vector_index_type: "hnsw".to_string(),
            vectorizer: "none".to_string(),
        },
    );
    WeaviateCollectionConfig {
        class_name,
        description: Some("Image vector storage".to_string()),
        properties: vec![
            text_property("media_id", "Persisted media ID"),
            text_property("original_source", "Original source string"),
            text_property("rustfs_path", "RustFS object path"),
            text_property("name", "Media name"),
            text_property("description", "Image summary description"),
            text_property("mime_type", "Media MIME type"),
            text_property("source", "Source tag, e.g. upload/qq_chat/web_search"),
        ],
        vectorizer: None,
        vector_config: Some(vector_config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_collection_schema_matches_persisted_media_fields() {
        let config = image_vector_collection_config("ImageCollection".to_string());
        let fields = config
            .properties
            .iter()
            .map(|property| property.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            vec![
                "media_id",
                "original_source",
                "rustfs_path",
                "name",
                "description",
                "mime_type",
                "source"
            ]
        );
    }
}
