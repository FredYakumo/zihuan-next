use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use crate::llm::function_tools::FunctionTool;
use crate::bot_adapter::adapter::SharedBotAdapter;
use crate::bot_adapter::models::event_model::MessageEvent;
use crate::bot_adapter::models::message::MessageProp;

/// Redis connection configuration, passed between nodes as a reference
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
}

/// MySQL connection configuration, passed between nodes as a reference
#[derive(Debug, Clone)]
pub struct MySqlConfig {
    pub url: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
}

/// Dataflow datatype. Use for checking compatibility between ports.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum DataType {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    Json,
    Binary,
    Vec(Box<DataType>),
    MessageEvent,
    MessageProp,
    Message,
    QQMessage,
    FunctionTools,
    BotAdapterRef,
    RedisRef,
    MySqlRef,
    Password,
    Custom(String),
}

impl DataType {
    pub fn is_compatible_with(&self, other: &DataType) -> bool {
        match (self, other) {
            (DataType::Any, _) | (_, DataType::Any) => true,
            (DataType::Vec(left), DataType::Vec(right)) => left.is_compatible_with(right),
            _ => self == other,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Any => write!(f, "Any"),
            DataType::String => write!(f, "String"),
            DataType::Integer => write!(f, "Integer"),
            DataType::Float => write!(f, "Float"),
            DataType::Boolean => write!(f, "Boolean"),
            DataType::Json => write!(f, "Json"),
            DataType::Binary => write!(f, "Binary"),
            DataType::Vec(inner) => write!(f, "Vec<{}>", inner),
            DataType::MessageEvent => write!(f, "MessageEvent"),
            DataType::MessageProp => write!(f, "MessageProp"),
            DataType::Message => write!(f, "Message"),
            DataType::QQMessage => write!(f, "QQMessage"),
            DataType::FunctionTools => write!(f, "FunctionTools"),
            DataType::BotAdapterRef => write!(f, "BotAdapterRef"),
            DataType::RedisRef => write!(f, "RedisRef"),
            DataType::MySqlRef => write!(f, "MySqlRef"),
            DataType::Password => write!(f, "Password"),
            DataType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

impl<'de> serde::Deserialize<'de> for DataType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct DataTypeVisitor;

        impl DataTypeVisitor {
            fn from_str<E: de::Error>(s: &str) -> Result<DataType, E> {
                // Backward-compat: handle "Vec<Inner>" string format produced by Display
                if let Some(inner_str) = s.strip_prefix("Vec<").and_then(|t| t.strip_suffix('>')) {
                    let inner = DataTypeVisitor::from_str(inner_str)?;
                    return Ok(DataType::Vec(Box::new(inner)));
                }
                match s {
                    "Any" => Ok(DataType::Any),
                    "String" => Ok(DataType::String),
                    "Integer" => Ok(DataType::Integer),
                    "Float" => Ok(DataType::Float),
                    "Boolean" => Ok(DataType::Boolean),
                    "Json" => Ok(DataType::Json),
                    "Binary" => Ok(DataType::Binary),
                    "MessageEvent" => Ok(DataType::MessageEvent),
                    "MessageProp" => Ok(DataType::MessageProp),
                    "Message" => Ok(DataType::Message),
                    "QQMessage" => Ok(DataType::QQMessage),
                    "FunctionTools" => Ok(DataType::FunctionTools),
                    "BotAdapterRef" => Ok(DataType::BotAdapterRef),
                    "RedisRef" => Ok(DataType::RedisRef),
                    "MySqlRef" => Ok(DataType::MySqlRef),
                    "Password" => Ok(DataType::Password),
                    other => Err(de::Error::unknown_variant(
                        other,
                        &["Any", "String", "Integer", "Float", "Boolean", "Json",
                          "Binary", "Vec", "MessageEvent", "MessageProp", "Message",
                          "QQMessage", "FunctionTools", "BotAdapterRef", "RedisRef",
                          "MySqlRef", "Password", "Custom"],
                    )),
                }
            }
        }

        impl<'de> Visitor<'de> for DataTypeVisitor {
            type Value = DataType;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a DataType string or {\"Vec\":...} / {\"Custom\":...} object")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                DataTypeVisitor::from_str(v)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::missing_field("variant key"))?;
                match key.as_str() {
                    "Vec" => {
                        let inner: DataType = map.next_value()?;
                        Ok(DataType::Vec(Box::new(inner)))
                    }
                    "Custom" => {
                        let name: String = map.next_value()?;
                        Ok(DataType::Custom(name))
                    }
                    other => Err(de::Error::unknown_variant(other, &["Vec", "Custom"])),
                }
            }
        }

        deserializer.deserialize_any(DataTypeVisitor)
    }
}

/// Actual data flowing through the dataflow graph
#[derive(Clone)]
pub enum DataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(Value),
    Binary(Vec<u8>),
    Vec(Box<DataType>, std::vec::Vec<DataValue>),
    MessageEvent(MessageEvent),
    Message(crate::llm::Message),
    QQMessage(crate::bot_adapter::models::message::Message),
    MessageProp(MessageProp),
    FunctionTools(Vec<Arc<dyn FunctionTool>>),
    BotAdapterRef(SharedBotAdapter),
    RedisRef(Arc<RedisConfig>),
    MySqlRef(Arc<MySqlConfig>),
    Password(String),
}

impl DataValue {
    pub fn data_type(&self) -> DataType {
        match self {
            DataValue::String(_) => DataType::String,
            DataValue::Integer(_) => DataType::Integer,
            DataValue::Float(_) => DataType::Float,
            DataValue::Boolean(_) => DataType::Boolean,
            DataValue::Json(_) => DataType::Json,
            DataValue::Binary(_) => DataType::Binary,
            DataValue::Vec(ty, _) => DataType::Vec(ty.clone()),
            DataValue::Message(_) => DataType::Message,
            DataValue::QQMessage(_) => DataType::QQMessage,
            DataValue::MessageEvent(_) => DataType::MessageEvent,
            DataValue::MessageProp(_) => DataType::MessageProp,
            DataValue::FunctionTools(_) => DataType::FunctionTools,
            DataValue::BotAdapterRef(_) => DataType::BotAdapterRef,
            DataValue::RedisRef(_) => DataType::RedisRef,
            DataValue::MySqlRef(_) => DataType::MySqlRef,
            DataValue::Password(_) => DataType::Password,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            DataValue::String(s) => Value::String(s.clone()),
            DataValue::Integer(i) => Value::Number((*i).into()),
            DataValue::Float(f) => serde_json::json!(f),
            DataValue::Boolean(b) => Value::Bool(*b),
            DataValue::Json(v) => v.clone(),
            DataValue::Binary(bytes) => Value::Array(bytes.iter().map(|b| Value::Number((*b).into())).collect()),
            DataValue::Vec(_, items) => {
                Value::Array(items.iter().map(|item| item.to_json()).collect())
            }
            DataValue::Message(m) => {
                serde_json::json!({
                    "role": crate::llm::role_to_str(&m.role),
                    "content": m.content,
                    "tool_calls": m.tool_calls,
                })
            }
            DataValue::QQMessage(m) => serde_json::to_value(m).unwrap_or(Value::Null),
            DataValue::MessageEvent(event) => {
                serde_json::json!({
                    "message_id": event.message_id,
                    "message_type": event.message_type.as_str(),
                    "sender": {
                        "user_id": event.sender.user_id,
                        "nickname": event.sender.nickname,
                        "card": event.sender.card,
                        "role": event.sender.role,
                    },
                    "group_id": event.group_id,
                    "group_name": event.group_name,
                    "is_group_message": event.is_group_message,
                })
            }
            DataValue::MessageProp(prop) => serde_json::json!({
                "content": prop.content,
                "ref_content": prop.ref_content,
                "is_at_me": prop.is_at_me,
                "at_target_list": prop.at_target_list,
            }),
            DataValue::FunctionTools(tools) => {
                let tool_defs: Vec<Value> = tools.iter()
                    .map(|t| t.get_json())
                    .collect();
                Value::Array(tool_defs)
            }
            DataValue::Password(value) => Value::String(value.clone()),
            DataValue::BotAdapterRef(_) => Value::String("BotAdapterRef".to_string()),
            DataValue::RedisRef(config) => serde_json::json!({
                "type": "RedisRef",
                "url": config.url,
                "reconnect_max_attempts": config.reconnect_max_attempts,
                "reconnect_interval_secs": config.reconnect_interval_secs,
            }),
            DataValue::MySqlRef(config) => serde_json::json!({
                "type": "MySqlRef",
                "url": config.url,
                "reconnect_max_attempts": config.reconnect_max_attempts,
                "reconnect_interval_secs": config.reconnect_interval_secs,
            }),
        }
    }
}

impl fmt::Debug for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValue::String(value) => f.debug_tuple("String").field(value).finish(),
            DataValue::Integer(value) => f.debug_tuple("Integer").field(value).finish(),
            DataValue::Float(value) => f.debug_tuple("Float").field(value).finish(),
            DataValue::Boolean(value) => f.debug_tuple("Boolean").field(value).finish(),
            DataValue::Json(value) => f.debug_tuple("Json").field(value).finish(),
            DataValue::Binary(value) => f.debug_tuple("Binary").field(value).finish(),
            DataValue::Vec(ty, value) => f.debug_tuple("Vec").field(ty).field(value).finish(),
            DataValue::Message(value) => f.debug_tuple("Message").field(value).finish(),
            DataValue::QQMessage(value) => f.debug_tuple("QQMessage").field(value).finish(),
            DataValue::MessageEvent(value) => f.debug_tuple("MessageEvent").field(value).finish(),
            DataValue::MessageProp(value) => f.debug_tuple("MessageProp").field(value).finish(),
            DataValue::FunctionTools(value) => f.debug_tuple("FunctionTools").field(value).finish(),
            DataValue::BotAdapterRef(_) => f.debug_tuple("BotAdapterRef").finish(),
            DataValue::RedisRef(config) => f.debug_tuple("RedisRef").field(config).finish(),
            DataValue::MySqlRef(config) => f.debug_tuple("MySqlRef").field(config).finish(),
            DataValue::Password(value) => f.debug_tuple("Password").field(value).finish(),
        }
    }
}

impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_json().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::DataType;

    #[test]
    fn any_type_is_compatible_with_concrete_types() {
        assert!(DataType::Any.is_compatible_with(&DataType::String));
        assert!(DataType::MessageEvent.is_compatible_with(&DataType::Any));
        assert!(DataType::Any.is_compatible_with(&DataType::Vec(Box::new(DataType::Integer))));
    }

    #[test]
    fn concrete_types_remain_strict() {
        assert!(DataType::String.is_compatible_with(&DataType::String));
        assert!(!DataType::String.is_compatible_with(&DataType::Integer));
        assert!(!DataType::Vec(Box::new(DataType::String))
            .is_compatible_with(&DataType::Vec(Box::new(DataType::Integer))));
    }
}
