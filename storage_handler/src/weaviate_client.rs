use std::collections::HashMap;

use serde_json::{json, Value};

use zihuan_core::error::{Error, Result};
use zihuan_core::weaviate::{
    gql_escape, graphql_value, WeaviateCollectionConfig, WeaviateObjectInput, WeaviateRef,
};

pub trait WeaviateClient {
    fn list_collections(&self) -> Result<Vec<String>>;

    fn collection_exists(&self, class_name: &str) -> Result<bool>;

    fn create_collection(&self, collection: &WeaviateCollectionConfig) -> Result<Value>;

    fn ensure_collection(&self, collection: &WeaviateCollectionConfig) -> Result<()>;

    fn find_collection_schema(&self, class_name: &str) -> Result<Option<Value>>;

    fn delete_collection(&self, class_name: &str) -> Result<()>;

    fn upsert_object(
        &self,
        class_name: &str,
        properties: Value,
        vector: Option<Vec<f32>>,
        id: Option<&str>,
    ) -> Result<Value>;

    fn upsert_object_with_vectors(
        &self,
        class_name: &str,
        properties: Value,
        vectors: HashMap<String, Vec<f32>>,
        id: Option<&str>,
    ) -> Result<Value>;

    fn batch_upsert_objects(&self, objects: &[WeaviateObjectInput]) -> Result<Value>;

    fn get_object(&self, class_name: &str, id: &str) -> Result<Value>;

    fn delete_object(&self, class_name: &str, id: &str) -> Result<()>;

    fn update_object(&self, class_name: &str, id: &str, properties: Value) -> Result<Value>;

    fn update_object_with_vector(
        &self,
        class_name: &str,
        id: &str,
        properties: Value,
        vector: Vec<f32>,
    ) -> Result<Value>;

    fn get_object_vector(&self, class_name: &str, id: &str) -> Result<Option<Vec<f32>>>;

    fn query_hybrid(
        &self,
        class_name: &str,
        query: &str,
        limit: usize,
        property_names: &[String],
        target_vector: Option<&str>,
        where_filter: Option<Value>,
        sort: Option<Value>,
        include_distance: bool,
    ) -> Result<Value>;

    fn query_all(
        &self,
        class_name: &str,
        limit: usize,
        property_names: &[String],
    ) -> Result<Value>;

    fn query_with_args(
        &self,
        class_name: &str,
        args: &str,
        property_names: &[String],
    ) -> Result<Value>;

    fn query_near_vector(
        &self,
        class_name: &str,
        vector: &[f32],
        target_vector: Option<&str>,
        limit: usize,
        property_names: &[String],
        include_distance: bool,
        include_vector: bool,
    ) -> Result<Value>;
}

impl WeaviateClient for WeaviateRef {
    fn list_collections(&self) -> Result<Vec<String>> {
        let schema = self.schema()?;
        let classes = schema
            .get("classes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(classes
            .into_iter()
            .filter_map(|class| {
                class
                    .get("class")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect())
    }

    fn collection_exists(&self, class_name: &str) -> Result<bool> {
        Ok(self
            .list_collections()?
            .iter()
            .any(|existing| existing == class_name))
    }

    fn create_collection(&self, collection: &WeaviateCollectionConfig) -> Result<Value> {
        self.post_json("/v1/schema", serde_json::to_value(collection)?)
    }

    fn ensure_collection(&self, collection: &WeaviateCollectionConfig) -> Result<()> {
        if self.collection_exists(&collection.class_name)? {
            return Ok(());
        }

        self.create_collection(collection)?;
        Ok(())
    }

    fn find_collection_schema(&self, class_name: &str) -> Result<Option<Value>> {
        let schema = self.schema()?;
        let classes = schema
            .get("classes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(classes.into_iter().find(|class| {
            class
                .get("class")
                .and_then(Value::as_str)
                .map(|name| name == class_name)
                .unwrap_or(false)
        }))
    }

    fn delete_collection(&self, class_name: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/schema/{class_name}"))
    }

    fn upsert_object(
        &self,
        class_name: &str,
        properties: Value,
        vector: Option<Vec<f32>>,
        id: Option<&str>,
    ) -> Result<Value> {
        let mut payload = json!({
            "class": class_name,
            "properties": properties,
        });
        if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
            payload["id"] = Value::String(id.to_string());
        }
        if let Some(vector) = vector {
            payload["vector"] = serde_json::to_value(vector)?;
        }
        self.post_json("/v1/objects", payload)
    }

    fn upsert_object_with_vectors(
        &self,
        class_name: &str,
        properties: Value,
        vectors: HashMap<String, Vec<f32>>,
        id: Option<&str>,
    ) -> Result<Value> {
        let mut payload = json!({
            "class": class_name,
            "properties": properties,
        });
        if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
            payload["id"] = Value::String(id.to_string());
        }
        if !vectors.is_empty() {
            payload["vectors"] = serde_json::to_value(vectors)?;
        }
        self.post_json("/v1/objects", payload)
    }

    fn batch_upsert_objects(&self, objects: &[WeaviateObjectInput]) -> Result<Value> {
        self.post_json("/v1/batch/objects", json!({ "objects": objects }))
    }

    fn get_object(&self, class_name: &str, id: &str) -> Result<Value> {
        self.get_json(&format!("/v1/objects/{class_name}/{id}"))
    }

    fn delete_object(&self, class_name: &str, id: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/objects/{class_name}/{id}"))
    }

    fn update_object(&self, class_name: &str, id: &str, properties: Value) -> Result<Value> {
        self.put_json(&format!("/v1/objects/{class_name}/{id}"), json!({
            "class": class_name,
            "id": id,
            "properties": properties,
        }))
    }

    fn update_object_with_vector(
        &self,
        class_name: &str,
        id: &str,
        properties: Value,
        vector: Vec<f32>,
    ) -> Result<Value> {
        self.put_json(&format!("/v1/objects/{class_name}/{id}"), json!({
            "class": class_name,
            "id": id,
            "properties": properties,
            "vector": vector,
        }))
    }

    fn get_object_vector(&self, class_name: &str, id: &str) -> Result<Option<Vec<f32>>> {
        self.get_object_vector(class_name, id)
    }

    fn query_hybrid(
        &self,
        class_name: &str,
        query: &str,
        limit: usize,
        property_names: &[String],
        target_vector: Option<&str>,
        where_filter: Option<Value>,
        sort: Option<Value>,
        include_distance: bool,
    ) -> Result<Value> {
        query_hybrid_impl(
            self,
            class_name,
            query,
            limit,
            property_names,
            target_vector,
            where_filter,
            sort,
            include_distance,
        )
    }

    fn query_all(
        &self,
        class_name: &str,
        limit: usize,
        property_names: &[String],
    ) -> Result<Value> {
        self.query_with_args(class_name, &format!("limit: {limit}"), property_names)
    }

    fn query_with_args(
        &self,
        class_name: &str,
        args: &str,
        property_names: &[String],
    ) -> Result<Value> {
        let mut requested_fields = property_names
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        requested_fields.push("_additional { id }".to_string());
        let fields = requested_fields.join(" ");
        let graphql = if args.trim().is_empty() {
            format!("{{ Get {{ {class_name} {{ {fields} }} }} }}")
        } else {
            format!("{{ Get {{ {class_name}({args}) {{ {fields} }} }} }}")
        };
        self.execute_graphql_query(&graphql)
    }

    fn query_near_vector(
        &self,
        class_name: &str,
        vector: &[f32],
        target_vector: Option<&str>,
        limit: usize,
        property_names: &[String],
        include_distance: bool,
        include_vector: bool,
    ) -> Result<Value> {
        let mut requested_fields = property_names
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        let mut additional_fields = vec!["id".to_string()];
        if include_distance {
            additional_fields.push("distance".to_string());
        }
        if include_vector {
            additional_fields.push("vector".to_string());
        }
        requested_fields.push(format!("_additional {{ {} }}", additional_fields.join(" ")));
        let vector_body = vector
            .iter()
            .map(|value| {
                let mut rendered = value.to_string();
                if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
                    rendered.push_str(".0");
                }
                rendered
            })
            .collect::<Vec<_>>()
            .join(", ");
        let fields = requested_fields.join(" ");
        let target_clause = target_vector
            .map(|tv| format!(r#", targetVectors: ["{}"]"#, tv))
            .unwrap_or_default();
        let graphql = format!(
            "{{ Get {{ {class_name}(nearVector: {{ vector: [{vector_body}]{target_clause} }}, limit: {limit}) {{ {fields} }} }} }}"
        );
        self.execute_graphql_query(&graphql)
    }
}

fn query_hybrid_impl(
    weaviate_ref: &WeaviateRef,
    class_name: &str,
    query: &str,
    limit: usize,
    property_names: &[String],
    target_vector: Option<&str>,
    where_filter: Option<Value>,
    sort: Option<Value>,
    include_distance: bool,
) -> Result<Value> {
    let mut requested_fields = property_names
        .iter()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let mut additional_fields = vec!["id".to_string()];
    if include_distance {
        additional_fields.push("distance".to_string());
    }
    requested_fields.push(format!("_additional {{ {} }}", additional_fields.join(" ")));
    let fields = requested_fields.join(" ");
    let hybrid_query = gql_escape(query);
    let where_clause = where_filter
        .map(|value| format!(", where: {}", graphql_value(&value)))
        .unwrap_or_default();
    let sort_clause = sort
        .map(|value| format!(", sort: {}", graphql_value(&value)))
        .unwrap_or_default();
    let target_clause = target_vector
        .map(|tv| format!(r#", targetVectors: ["{}"]"#, gql_escape(tv)))
        .unwrap_or_default();
    let graphql = format!(
        "{{ Get {{ {class_name}(hybrid: {{ query: \"{hybrid_query}\"{target_clause} }}, limit: {limit}{where_clause}{sort_clause}) {{ {fields} }} }} }}"
    );
    weaviate_ref.execute_graphql_query(&graphql)
}

