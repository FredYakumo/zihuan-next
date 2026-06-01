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
    ImageSemantic,
    AgentMemory,
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
    pub client: Client,
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

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    pub fn authorized(&self, builder: RequestBuilder) -> RequestBuilder {
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

    pub fn get_json(&self, path: &str) -> Result<Value> {
        crate::runtime::block_async(self.get_json_async(path))
    }

    pub fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        crate::runtime::block_async(self.post_json_async(path, body))
    }

    pub fn put_json(&self, path: &str, body: Value) -> Result<Value> {
        crate::runtime::block_async(self.put_json_async(path, body))
    }

    pub fn delete_empty(&self, path: &str) -> Result<()> {
        crate::runtime::block_async(self.delete_empty_async(path))
    }

    pub async fn ready_async(&self) -> Result<bool> {
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

    pub async fn get_json_async(&self, path: &str) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client.get(self.url(path)))).await
    }

    pub async fn post_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(
            self.authorized(self.client.post(self.url(path)))
                .json(&body),
        )
        .await
    }

    pub async fn put_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client.put(self.url(path))).json(&body)).await
    }

    pub async fn delete_empty_async(&self, path: &str) -> Result<()> {
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

    pub fn get_object_vector(&self, class_name: &str, id: &str) -> Result<Option<Vec<f32>>> {
        crate::runtime::block_async(self.get_object_vector_async(class_name, id))
    }

    pub async fn get_object_vector_async(
        &self,
        class_name: &str,
        id: &str,
    ) -> Result<Option<Vec<f32>>> {
        let query = format!(
            r#"{{ Get {{ {class_name}(where: {{path: ["_id"], operator: Equal, valueText: "{id}"}}) {{ _additional {{ vector }} }} }} }}"#
        );
        let response = self.execute_graphql_query(&query)?;
        let items = response
            .get("data")
            .and_then(|d| d.get("Get"))
            .and_then(|g| g.get(class_name))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        match items.first() {
            Some(item) => Ok(parse_vector_from_additional(item)),
            None => Ok(None),
        }
    }

    pub async fn send_json_async(builder: RequestBuilder) -> Result<Value> {
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

pub fn gql_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

pub fn graphql_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", gql_escape(s)),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => {
            let rendered = items
                .iter()
                .map(graphql_value)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", rendered)
        }
        Value::Object(map) => {
            let rendered = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, graphql_value(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", rendered)
        }
        Value::Null => "null".to_string(),
    }
}

fn parse_vector_from_additional(item: &Value) -> Option<Vec<f32>> {
    item.get("_additional")?
        .get("vector")?
        .as_array()?
        .iter()
        .map(|v| v.as_f64().map(|f| f as f32))
        .collect()
}
