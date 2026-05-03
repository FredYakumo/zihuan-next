use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;
use zihuan_core::error::{Error, Result};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Clone)]
pub struct WeaviateRef {
    pub base_url: String,
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
pub struct WeaviateCollectionConfig {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<WeaviatePropertyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vectorizer: Option<String>,
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
}

impl WeaviateRef {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self> {
        let base_url = normalize_base_url(base_url.into())?;
        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            base_url,
            api_key: api_key.filter(|value| !value.trim().is_empty()),
            timeout,
            client,
        })
    }

    pub fn ready(&self) -> Result<bool> {
        let response = self
            .authorized(self.client.get(self.url("/v1/.well-known/ready")))
            .send()?;

        if response.status().is_success() {
            return Ok(true);
        }

        if response.status().as_u16() == 503 {
            return Ok(false);
        }

        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate readiness probe failed with status {}: {}",
            status, body
        )))
    }

    pub fn meta(&self) -> Result<Value> {
        self.get_json("/v1/meta")
    }

    pub fn schema(&self) -> Result<Value> {
        self.get_json("/v1/schema")
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

    pub fn batch_upsert_objects(&self, objects: &[WeaviateObjectInput]) -> Result<Value> {
        self.post_json(
            "/v1/batch/objects",
            json!({
                "objects": objects,
            }),
        )
    }

    pub fn get_object(&self, class_name: &str, id: &str) -> Result<Value> {
        self.get_json(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn delete_object(&self, class_name: &str, id: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn query_near_vector(
        &self,
        class_name: &str,
        vector: &[f32],
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
        let graphql = format!(
            "{{ Get {{ {class_name}(nearVector: {{ vector: [{vector_body}] }}, limit: {limit}) {{ {fields} }} }} }}"
        );

        self.post_json("/v1/graphql", json!({ "query": graphql }))
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn authorized(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = &self.api_key {
            builder.bearer_auth(api_key)
        } else {
            builder
        }
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        self.send_json(self.authorized(self.client.get(self.url(path))))
    }

    fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        self.send_json(
            self.authorized(self.client.post(self.url(path)))
                .json(&body),
        )
    }

    fn delete_empty(&self, path: &str) -> Result<()> {
        let response = self
            .authorized(self.client.delete(self.url(path)))
            .send()
            .map_err(Error::from)?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status();
        let body = response.text().unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate request failed with status {}: {}",
            status, body
        )))
    }

    fn send_json(&self, builder: RequestBuilder) -> Result<Value> {
        let response = builder.send()?;
        let status = response.status();
        let body = response.text()?;

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
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

pub struct WeaviateNode {
    id: String,
    name: String,
}

impl WeaviateNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for WeaviateNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Weaviate 向量数据库配置，输出 WeaviateRef 引用供下游节点复用")
    }

    node_input![
        port! { name = "base_url", ty = String, desc = "Weaviate HTTP 地址，例如 http://127.0.0.1:8080" },
        port! { name = "api_key", ty = Password, desc = "可选：Weaviate API Key", optional },
        port! { name = "timeout_secs", ty = Integer, desc = "可选：请求超时秒数，默认 30 秒", optional },
    ];

    node_output![port! { name = "weaviate_ref", ty = WeaviateRef, desc = "Weaviate 数据库引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let base_url = inputs
            .get("base_url")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.trim().to_string()),
                _ => None,
            })
            .ok_or_else(|| Error::ValidationError("base_url is required".to_string()))?;
        if base_url.is_empty() {
            return Err(Error::ValidationError(
                "base_url must not be empty".to_string(),
            ));
        }

        let api_key = inputs.get("api_key").and_then(|value| match value {
            DataValue::Password(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let timeout_secs = match inputs.get("timeout_secs") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as u64,
            Some(DataValue::Integer(_)) | None => DEFAULT_TIMEOUT_SECS,
            Some(_) => {
                return Err(Error::ValidationError(
                    "timeout_secs must be an integer".to_string(),
                ))
            }
        };

        let weaviate_ref = Arc::new(WeaviateRef::new(
            base_url,
            api_key,
            Duration::from_secs(timeout_secs),
        )?);

        if !weaviate_ref.ready()? {
            return Err(Error::StringError(
                "Weaviate is reachable but not ready yet".to_string(),
            ));
        }

        let outputs = HashMap::from([(
            "weaviate_ref".to_string(),
            DataValue::WeaviateRef(weaviate_ref),
        )]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
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
