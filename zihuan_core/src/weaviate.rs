use crate::connection::Connection;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use crate::error::{Error, Result};

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
    client: Arc<RwLock<Option<Client>>>,
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
        let username = normalize_owned_optional_string(username);
        let password = normalize_owned_optional_string(password);
        let api_key = normalize_owned_optional_string(api_key);
        if username.is_some() != password.is_some() {
            return Err(Error::ValidationError(
                "weaviate username and password must be supplied together".to_string(),
            ));
        }
        if api_key.is_some() && username.is_some() {
            return Err(Error::ValidationError(
                "configure either weaviate username/password or api_key, not both".to_string(),
            ));
        }
        if api_key.is_none() && username.is_none() {
            return Err(Error::ValidationError("weaviate authentication is required".to_string()));
        }
        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            base_url,
            class_name,
            username,
            password,
            api_key,
            timeout,
            client: Arc::new(RwLock::new(Some(client))),
        })
    }

    fn client(&self) -> Result<Client> {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| crate::string_error!("Weaviate connection is closed"))
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
        match (&self.api_key, &self.username, &self.password) {
            (Some(api_key), None, None) => builder.bearer_auth(api_key),
            (None, Some(username), Some(password)) => builder.basic_auth(username, Some(password)),
            _ => builder,
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
            .authorized(self.client()?.get(self.url("/v1/.well-known/ready")))
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(true);
        }
        if response.status().as_u16() == 503 {
            return Ok(false);
        }
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(Error::NotFound);
        }
        let body = response.text().await.unwrap_or_default();
        Err(crate::string_error!(
            "Weaviate readiness probe failed with status {}: {}",
            status,
            body
        ))
    }

    pub async fn get_json_async(&self, path: &str) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client()?.get(self.url(path)))).await
    }

    pub async fn post_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client()?.post(self.url(path))).json(&body))
            .await
    }

    pub async fn put_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client()?.put(self.url(path))).json(&body)).await
    }

    pub async fn delete_empty_async(&self, path: &str) -> Result<()> {
        let response = self.authorized(self.client()?.delete(self.url(path))).send().await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        if status.as_u16() == 404 {
            return Err(Error::NotFound);
        }
        let body = response.text().await.unwrap_or_default();
        Err(crate::string_error!("Weaviate request failed with status {}: {}", status, body))
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
        let response = self.post_json_async("/v1/graphql", json!({ "query": query })).await?;
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
        if status.as_u16() == 404 {
            return Err(Error::NotFound);
        }
        let body = response.text().await?;
        if !status.is_success() {
            return Err(crate::string_error!(
                "Weaviate request failed with status {}: {}",
                status,
                body
            ));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|err| {
            crate::string_error!("Failed to parse Weaviate response as JSON: {err}; body={body}")
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
        return Err(Error::ValidationError("Weaviate base_url must not be empty".to_string()));
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
        return Err(Error::ValidationError("Weaviate class_name must not be empty".to_string()));
    }
    Ok(trimmed)
}

fn normalize_owned_optional_string(value: Option<String>) -> Option<String> {
    value.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

pub fn gql_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Maps a Weaviate cosine distance (0 = identical, 2 = opposite) onto a
/// relevance in [0, 1] where higher means more relevant.
pub fn cosine_distance_relevance(distance: f64) -> f64 {
    (1.0 - distance).clamp(0.0, 1.0)
}

pub fn graphql_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", gql_escape(s)),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(items) => {
            let rendered = items.iter().map(graphql_value).collect::<Vec<_>>().join(", ");
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

#[async_trait::async_trait]
impl Connection for WeaviateRef {
    async fn connect(&mut self) -> Result<()> {
        if self.is_closed() {
            let client = Client::builder().timeout(self.timeout).build()?;
            *self.client.write().unwrap_or_else(|e| e.into_inner()) = Some(client);
        }
        if let Err(error) = self.health_check().await {
            self.close().await?;
            return Err(error);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        if self.ready_async().await? {
            Ok(())
        } else {
            Err(crate::string_error!("Weaviate is reachable but not ready yet"))
        }
    }

    async fn close(&self) -> Result<()> {
        self.client.write().unwrap_or_else(|e| e.into_inner()).take();
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.client.read().unwrap_or_else(|e| e.into_inner()).is_none()
    }
}

impl WeaviateRef {
    pub fn list_collections(&self) -> Result<Vec<String>> {
        let schema = self.schema()?;
        let classes = schema.get("classes").and_then(Value::as_array).cloned().unwrap_or_default();

        Ok(classes
            .into_iter()
            .filter_map(|class| class.get("class").and_then(Value::as_str).map(str::to_string))
            .collect())
    }

    pub fn collection_exists(&self, class_name: &str) -> Result<bool> {
        Ok(self.list_collections()?.iter().any(|existing| existing == class_name))
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
        let classes = schema.get("classes").and_then(Value::as_array).cloned().unwrap_or_default();

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

    pub fn update_object(&self, class_name: &str, id: &str, properties: Value) -> Result<Value> {
        self.put_json(
            &format!("/v1/objects/{class_name}/{id}"),
            json!({
                "class": class_name,
                "id": id,
                "properties": properties,
            }),
        )
    }

    pub fn update_object_with_vector(
        &self,
        class_name: &str,
        id: &str,
        properties: Value,
        vector: Vec<f32>,
    ) -> Result<Value> {
        self.put_json(
            &format!("/v1/objects/{class_name}/{id}"),
            json!({
                "class": class_name,
                "id": id,
                "properties": properties,
                "vector": vector,
            }),
        )
    }

    pub fn query_hybrid(
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

    pub fn query_all(
        &self,
        class_name: &str,
        limit: usize,
        property_names: &[String],
    ) -> Result<Value> {
        self.query_with_args(class_name, &format!("limit: {limit}"), property_names, false)
    }

    pub fn query_with_args(
        &self,
        class_name: &str,
        args: &str,
        property_names: &[String],
        include_score: bool,
    ) -> Result<Value> {
        let mut requested_fields = property_names
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        // `score` is only populated by scored queries (bm25/hybrid); plain listings
        // leave it out so Weaviate never has to interpret it against a sort clause.
        let additional_fields = if include_score { "id score" } else { "id" };
        requested_fields.push(format!("_additional {{ {additional_fields} }}"));
        let fields = requested_fields.join(" ");
        let graphql = if args.trim().is_empty() {
            format!("{{ Get {{ {class_name} {{ {fields} }} }} }}")
        } else {
            format!("{{ Get {{ {class_name}({args}) {{ {fields} }} }} }}")
        };
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
