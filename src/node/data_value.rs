use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use crate::llm::{Message, function_tools::FunctionTool};
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataType {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    Json,
    Binary,
    List(Box<DataType>),
    MessageList,
    QQMessageList,
    MessageEvent,
    MessageProp,
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
            (DataType::List(left), DataType::List(right)) => left.is_compatible_with(right),
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
            DataType::List(inner) => write!(f, "List<{}>", inner),
            DataType::MessageList => write!(f, "MessageList"),
            DataType::QQMessageList => write!(f, "QQMessageList"),
            DataType::MessageEvent => write!(f, "MessageEvent"),
            DataType::MessageProp => write!(f, "MessageProp"),
            DataType::FunctionTools => write!(f, "FunctionTools"),
            DataType::BotAdapterRef => write!(f, "BotAdapterRef"),
            DataType::RedisRef => write!(f, "RedisRef"),
            DataType::MySqlRef => write!(f, "MySqlRef"),
            DataType::Password => write!(f, "Password"),
            DataType::Custom(name) => write!(f, "Custom({})", name),
        }
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
    List(Vec<DataValue>),
    MessageList(Vec<Message>),
    QQMessageList(Vec<crate::bot_adapter::models::message::Message>),
    MessageEvent(MessageEvent),
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
            DataValue::List(items) => {
                if let Some(first) = items.first() {
                    DataType::List(Box::new(first.data_type()))
                } else {
                    DataType::List(Box::new(DataType::String))
                }
            }
            DataValue::MessageList(_) => DataType::MessageList,
            DataValue::QQMessageList(_) => DataType::QQMessageList,
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
            DataValue::List(items) => {
                Value::Array(items.iter().map(|item| item.to_json()).collect())
            }
            DataValue::QQMessageList(messages) => {
                Value::Array(messages.iter().map(|m| serde_json::to_value(m).unwrap_or(Value::Null)).collect())
            }
            DataValue::MessageList(messages) => {
                let msgs: Vec<Value> = messages.iter().map(|m| {
                    serde_json::json!({
                        "role": crate::llm::role_to_str(&m.role),
                        "content": m.content,
                        "tool_calls": m.tool_calls,
                    })
                }).collect();
                Value::Array(msgs)
            }
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
            DataValue::List(value) => f.debug_tuple("List").field(value).finish(),
            DataValue::MessageList(value) => f.debug_tuple("MessageList").field(value).finish(),
            DataValue::QQMessageList(value) => f.debug_tuple("QQMessageList").field(value).finish(),
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
        assert!(DataType::Any.is_compatible_with(&DataType::List(Box::new(DataType::Integer))));
    }

    #[test]
    fn concrete_types_remain_strict() {
        assert!(DataType::String.is_compatible_with(&DataType::String));
        assert!(!DataType::String.is_compatible_with(&DataType::Integer));
        assert!(!DataType::List(Box::new(DataType::String))
            .is_compatible_with(&DataType::List(Box::new(DataType::Integer))));
    }
}
