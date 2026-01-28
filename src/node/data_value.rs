use serde::Serialize;
use serde_json::Value;
use std::fmt;
use std::sync::Arc;
use crate::llm::{Message, function_tools::FunctionTool};

/// Dataflow datatype. Use for checking compatibility between ports.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Json,
    Binary,
    List(Box<DataType>),
    MessageList,
    FunctionTools,
    Custom(String),
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::String => write!(f, "String"),
            DataType::Integer => write!(f, "Integer"),
            DataType::Float => write!(f, "Float"),
            DataType::Boolean => write!(f, "Boolean"),
            DataType::Json => write!(f, "Json"),
            DataType::Binary => write!(f, "Binary"),
            DataType::List(inner) => write!(f, "List<{}>", inner),
            DataType::MessageList => write!(f, "MessageList"),
            DataType::FunctionTools => write!(f, "FunctionTools"),
            DataType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// Actual data flowing through the dataflow graph
#[derive(Debug, Clone)]
pub enum DataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(Value),
    Binary(Vec<u8>),
    List(Vec<DataValue>),
    MessageList(Vec<Message>),
    FunctionTools(Vec<Arc<dyn FunctionTool>>),
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
            DataValue::FunctionTools(_) => DataType::FunctionTools,
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
            DataValue::FunctionTools(tools) => {
                let tool_defs: Vec<Value> = tools.iter()
                    .map(|t| t.get_json())
                    .collect();
                Value::Array(tool_defs)
            }
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
