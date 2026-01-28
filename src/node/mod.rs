use serde_json::{json, Value};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use crate::error::Result;

pub mod util_nodes;

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
            DataType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

/// Node input/output ports
#[derive(Debug, Clone, Serialize)]
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub description: Option<String>,
}

impl Port {
    pub fn new(name: impl Into<String>, data_type: DataType) -> Self {
        Self {
            name: name.into(),
            data_type,
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Actual data flowing through the dataflow graph
#[derive(Debug, Clone, Serialize)]
pub enum DataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(Value),
    Binary(Vec<u8>),
    List(Vec<DataValue>),
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
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// 连接两个节点的输入输出
#[derive(Debug, Clone, Serialize)]
pub struct Link {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
    pub data_type: DataType,
}

impl Link {
    /// 创建新的连接
    pub fn new(
        from_node: impl Into<String>,
        from_port: impl Into<String>,
        to_node: impl Into<String>,
        to_port: impl Into<String>,
        data_type: DataType,
    ) -> Self {
        Self {
            from_node: from_node.into(),
            from_port: from_port.into(),
            to_node: to_node.into(),
            to_port: to_port.into(),
            data_type,
        }
    }

    /// 检查类型是否匹配
    pub fn is_type_compatible(&self, from_type: &DataType, to_type: &DataType) -> bool {
        from_type == to_type || from_type == &self.data_type && to_type == &self.data_type
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// Node trait 定义节点的行为
pub trait Node: Send + Sync {
    /// 获取节点的唯一标识符
    fn id(&self) -> &str;

    /// 获取节点的名称
    fn name(&self) -> &str;

    /// 获取节点的描述
    fn description(&self) -> Option<&str> {
        None
    }

    /// 获取输入端口列表
    fn input_ports(&self) -> Vec<Port>;

    /// 获取输出端口列表
    fn output_ports(&self) -> Vec<Port>;

    /// 执行节点的主要逻辑
    /// inputs: 输入端口名称 -> 数据值
    /// 返回: 输出端口名称 -> 数据值
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;

    /// 将节点转换为 JSON 表示
    fn to_json(&self) -> Value {
        json!({
            "id": self.id(),
            "name": self.name(),
            "description": self.description(),
            "input_ports": serde_json::to_value(&self.input_ports()).unwrap_or(Value::Null),
            "output_ports": serde_json::to_value(&self.output_ports()).unwrap_or(Value::Null),
        })
    }

    /// 验证输入是否符合端口定义
    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> {
        let input_ports = self.input_ports();
        
        for port in &input_ports {
            if let Some(value) = inputs.get(&port.name) {
                if value.data_type() != port.data_type {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Input port '{}' expects type {}, got {}",
                        port.name,
                        port.data_type,
                        value.data_type()
                    )));
                }
            }
        }
        
        Ok(())
    }

    /// 验证输出是否符合端口定义
    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()> {
        let output_ports = self.output_ports();
        
        for port in &output_ports {
            if let Some(value) = outputs.get(&port.name) {
                if value.data_type() != port.data_type {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Output port '{}' expects type {}, got {}",
                        port.name,
                        port.data_type,
                        value.data_type()
                    )));
                }
            }
        }
        
        Ok(())
    }
}

/// 节点图，管理多个节点和它们之间的连接
pub struct NodeGraph {
    pub nodes: HashMap<String, Box<dyn Node>>,
    pub links: Vec<Link>,
}

impl NodeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            links: Vec::new(),
        }
    }

    /// 添加节点
    pub fn add_node(&mut self, node: Box<dyn Node>) -> Result<()> {
        let id = node.id().to_string();
        if self.nodes.contains_key(&id) {
            return Err(crate::error::Error::ValidationError(format!(
                "Node with id '{}' already exists",
                id
            )));
        }
        self.nodes.insert(id, node);
        Ok(())
    }

    /// 添加连接
    pub fn add_link(&mut self, link: Link) -> Result<()> {
        // 验证节点存在
        let from_node = self.nodes.get(&link.from_node).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "From node '{}' not found",
                link.from_node
            ))
        })?;

        let to_node = self.nodes.get(&link.to_node).ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "To node '{}' not found",
                link.to_node
            ))
        })?;

        // 验证端口存在和类型匹配
        let from_ports = from_node.output_ports();
        let from_port = from_ports
            .iter()
            .find(|p| p.name == link.from_port)
            .ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Output port '{}' not found in node '{}'",
                    link.from_port, link.from_node
                ))
            })?;

        let to_ports = to_node.input_ports();
        let to_port = to_ports
            .iter()
            .find(|p| p.name == link.to_port)
            .ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Input port '{}' not found in node '{}'",
                    link.to_port, link.to_node
                ))
            })?;

        // 验证类型匹配
        if from_port.data_type != to_port.data_type {
            return Err(crate::error::Error::ValidationError(format!(
                "Type mismatch: output port '{}' ({}) cannot connect to input port '{}' ({})",
                link.from_port, from_port.data_type, link.to_port, to_port.data_type
            )));
        }

        self.links.push(link);
        Ok(())
    }

    /// 执行节点图（简单的顺序执行）
    pub fn execute(&mut self) -> Result<()> {
        // TODO: 实现拓扑排序和依赖管理
        // 这里提供一个简化版本
        Ok(())
    }

    /// 转换为 JSON
    pub fn to_json(&self) -> Value {
        json!({
            "nodes": self.nodes.iter().map(|(id, node)| {
                json!({
                    "id": id,
                    "node": node.to_json(),
                })
            }).collect::<Vec<_>>(),
            "links": serde_json::to_value(&self.links).unwrap_or(Value::Null),
        })
    }
}

impl Default for NodeGraph {
    fn default() -> Self {
        Self::new()
    }
}