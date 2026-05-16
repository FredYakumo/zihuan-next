use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaviateCollectionSchema {
    MessageRecordSemantic,
    ImageSemantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaviateEnsureCollectionResult {
    Existing,
    Created,
}

#[derive(Clone)]
pub struct WeaviateRef {
    pub base_url: String,
    pub class_name: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
    pub timeout: Duration,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviatePropertyConfig {
    pub name: String,
    #[serde(rename = "dataType")]
    pub data_type: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaviateNamedVectorizerConfig {
    pub modules: HashMap<String, Value>,
}

impl WeaviateNamedVectorizerConfig {
    pub fn self_provided() -> Self {
        Self {
            modules: HashMap::from([("none".to_string(), json!({}))]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateVectorConfigEntry {
    #[serde(rename = "vectorIndexType")]
    pub vector_index_type: String,
    pub vectorizer: WeaviateNamedVectorizerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateCollectionConfig {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<WeaviatePropertyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vectorizer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "vectorConfig")]
    pub vector_config: Option<HashMap<String, WeaviateVectorConfigEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateObjectInput {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub properties: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec<f32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vectors: Option<HashMap<String, Vec<f32>>>,
}

impl WeaviateRef {
    pub fn new(
        base_url: impl Into<String>,
        class_name: impl Into<String>,
        username: Option<String>,
        password: Option<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self> {
        let base_url = normalize_base_url(base_url.into())?;
        let class_name = normalize_class_name(class_name.into())?;
        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            base_url,
            class_name,
            username: normalize_owned_optional_string(username),
            password: normalize_owned_optional_string(password),
            api_key: api_key.filter(|value| !value.trim().is_empty()),
            timeout,
            client,
        })
    }

    pub fn ready(&self) -> Result<bool> {
        crate::runtime::block_async(self.ready_async())
    }

    pub fn meta(&self) -> Result<Value> {
        self.get_json("/v1/meta")
    }

    pub fn schema(&self) -> Result<Value> {
        self.get_json("/v1/schema")
    }

    pub fn execute_graphql_query(&self, query: &str) -> Result<Value> {
        self.post_json("/v1/graphql", json!({ "query": query }))
    }

    pub fn list_collections(&self) -> Result<Vec<String>> {
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

    pub fn collection_exists(&self, class_name: &str) -> Result<bool> {
        Ok(self
            .list_collections()?
            .iter()
            .any(|existing| existing == class_name))
    }

    pub fn create_collection(&self, collection: &WeaviateCollectionConfig) -> Result<Value> {
        self.post_json("/v1/schema", serde_json::to_value(collection)?)
    }

    pub fn ensure_collection(&self, collection: &WeaviateCollectionConfig) -> Result<()> {
        if self.collection_exists(&collection.class_name)? {
            return Ok(());
        }

        self.create_collection(collection)?;
        Ok(())
    }

    pub fn find_collection_schema(&self, class_name: &str) -> Result<Option<Value>> {
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

    pub fn delete_collection(&self, class_name: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/schema/{class_name}"))
    }

    pub fn upsert_object(
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

    pub fn upsert_object_with_vectors(
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

    pub fn batch_upsert_objects(&self, objects: &[WeaviateObjectInput]) -> Result<Value> {
        self.post_json("/v1/batch/objects", json!({ "objects": objects }))
    }

    pub fn get_object(&self, class_name: &str, id: &str) -> Result<Value> {
        self.get_json(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn delete_object(&self, class_name: &str, id: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn query_all(
        &self,
        class_name: &str,
        limit: usize,
        property_names: &[String],
    ) -> Result<Value> {
        let mut requested_fields = property_names
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        requested_fields.push("_additional { id }".to_string());
        let fields = requested_fields.join(" ");
        let graphql = format!(
            "{{ Get {{ {class_name}(limit: {limit}) {{ {fields} }} }} }}"
        );
        self.execute_graphql_query(&graphql)
    }

    pub fn query_near_vector(
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

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn authorized(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = &self.api_key {
            builder.bearer_auth(api_key)
        } else if self.username.is_some() || self.password.is_some() {
            builder.basic_auth(
                self.username.clone().unwrap_or_default(),
                self.password.clone(),
            )
        } else {
            builder
        }
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        crate::runtime::block_async(self.get_json_async(path))
    }

    fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        crate::runtime::block_async(self.post_json_async(path, body))
    }

    fn delete_empty(&self, path: &str) -> Result<()> {
        crate::runtime::block_async(self.delete_empty_async(path))
    }

    async fn ready_async(&self) -> Result<bool> {
        let response = self
            .authorized(self.client.get(self.url("/v1/.well-known/ready")))
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(true);
        }
        if response.status().as_u16() == 503 {
            return Ok(false);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate readiness probe failed with status {}: {}",
            status, body
        )))
    }

    async fn get_json_async(&self, path: &str) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client.get(self.url(path)))).await
    }

    async fn post_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(
            self.authorized(self.client.post(self.url(path)))
                .json(&body),
        )
        .await
    }

    async fn delete_empty_async(&self, path: &str) -> Result<()> {
        let response = self
            .authorized(self.client.delete(self.url(path)))
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate request failed with status {}: {}",
            status, body
        )))
    }

    async fn send_json_async(builder: RequestBuilder) -> Result<Value> {
        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(Error::StringError(format!(
                "Weaviate request failed with status {}: {}",
                status, body
            )));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|err| {
            Error::StringError(format!(
                "Failed to parse Weaviate response as JSON: {err}; body={body}"
            ))
        })
    }
}

impl fmt::Debug for WeaviateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeaviateRef")
            .field("base_url", &self.base_url)
            .field("class_name", &self.class_name)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

fn normalize_base_url(raw: String) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "Weaviate base_url must not be empty".to_string(),
        ));
    }
    let parsed = reqwest::Url::parse(&trimmed).map_err(|err| {
        Error::ValidationError(format!("Invalid Weaviate base_url '{trimmed}': {err}"))
    })?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(Error::ValidationError(format!(
            "Weaviate base_url must use http or https, got '{scheme}'"
        )));
    }
    Ok(trimmed)
}

fn normalize_class_name(raw: String) -> Result<String> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "Weaviate class_name must not be empty".to_string(),
        ));
    }
    Ok(trimmed)
}

fn normalize_owned_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
