use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::agent::tool_config::AgentToolConfig;
use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleServiceConfig {
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
    pub name: String,
    pub role_service_type: RoleServiceType,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub tools: Vec<AgentToolConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl RoleServiceConfig {
    pub fn canonical_config_id(&self) -> &str {
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RoleServiceKind {
    QqChat,
    Workspace,
}

/// A concrete role service kind with its configuration payload.
///
/// `zihuan_core` does not depend back on the service crates, so it holds no concrete
/// QQ/Workspace config types here; instead it stores a typed [`RoleServiceKind`] alongside
/// the raw config object. The serialization format matches the historical internally-tagged
/// enum (`{"type": "qq_chat", ...}`); the concrete payload is parsed by the service crate
/// that owns the type via [`RoleServiceType::parse_typed_config`] and built via
/// [`RoleServiceType::from_typed_config`].
#[derive(Debug, Clone, PartialEq)]
pub struct RoleServiceType {
    pub kind: RoleServiceKind,
    payload: Map<String, Value>,
}

impl RoleServiceType {
    pub fn kind_tag(&self) -> &'static str {
        self.kind.into()
    }

    pub fn payload(&self) -> &Map<String, Value> {
        &self.payload
    }

    pub fn payload_mut(&mut self) -> &mut Map<String, Value> {
        &mut self.payload
    }

    /// Constructs from the config object of a registered type.
    pub fn from_typed_config<T: Serialize>(kind: RoleServiceKind, config: &T) -> Result<Self> {
        let kind_tag: &'static str = kind.into();
        let value = serde_json::to_value(config)?;
        let payload = value.as_object().cloned().ok_or_else(|| {
            crate::string_error!(
                "role service config for '{}' must serialize to an object",
                kind_tag
            )
        })?;
        Ok(Self { kind, payload })
    }

    /// Parses the current payload into a concrete config type; returns an error when the
    /// kind does not match or the payload is invalid.
    pub fn parse_typed_config<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        Ok(serde_json::from_value(Value::Object(self.payload.clone()))?)
    }

    /// The payload as a raw object (including the `"type"` field), used to round-trip
    /// byte-for-byte identically with the legacy enum format.
    pub fn as_value(&self) -> Value {
        let mut object = self.payload.clone();
        object.insert("type".to_string(), Value::String(self.kind.to_string()));
        Value::Object(object)
    }
}

impl Serialize for RoleServiceType {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_value().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RoleServiceType {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut object = Map::deserialize(deserializer)?;
        let kind = object
            .remove("type")
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                serde::de::Error::custom("role service type object must contain a non-empty 'type'")
            })?;
        let kind: RoleServiceKind = kind.parse().map_err(|_| {
            serde::de::Error::custom(format!("unknown role service type '{}'", kind))
        })?;
        Ok(Self { kind, payload: object })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBackendKind {
    LocalFile,
    Weaviate,
    Elasticsearch,
}
