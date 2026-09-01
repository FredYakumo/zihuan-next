use crate::error::Result;
use crate::graph::{DataType, DataValue, Node, NodeConfigField, NodeConfigFlow};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Node factory function type
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;
pub type RegistryInitFn = fn() -> Result<()>;

/// Global node registry
pub struct NodeRegistry {
    factories: RwLock<HashMap<String, NodeFactory>>,
    metadata: RwLock<HashMap<String, NodeTypeMetadata>>,
}

#[derive(Debug, Clone)]
pub struct NodeTypeMetadata {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
            metadata: RwLock::new(HashMap::new()),
        }
    }

    /// Register a node type with its factory function
    pub fn register(
        &self,
        type_id: impl Into<String>,
        display_name: impl Into<String>,
        category: impl Into<String>,
        description: impl Into<String>,
        factory: NodeFactory,
    ) -> Result<()> {
        let type_id = type_id.into();
        let metadata = NodeTypeMetadata {
            type_id: type_id.clone(),
            display_name: display_name.into(),
            category: category.into(),
            description: description.into(),
        };

        self.factories.write().unwrap().insert(type_id.clone(), factory);
        self.metadata.write().unwrap().insert(type_id, metadata);
        Ok(())
    }

    /// Create a new node instance by type ID
    pub fn create_node(
        &self,
        type_id: &str,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Box<dyn Node>> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id).ok_or_else(|| {
            crate::error::Error::ValidationError(format!("Node type '{}' not registered", type_id))
        })?;

        Ok(factory(id.into(), name.into()))
    }

    /// Return the canonical input and output ports for a registered node type.
    ///
    /// Returns `None` if the type is not registered.
    pub fn get_node_ports(
        &self,
        type_id: &str,
    ) -> Option<(Vec<crate::graph::Port>, Vec<crate::graph::Port>)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((node.input_ports(), node.output_ports()))
    }

    pub fn get_node_dynamic_port_flags(&self, type_id: &str) -> Option<(bool, bool)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((node.has_dynamic_input_ports(), node.has_dynamic_output_ports()))
    }

    pub fn get_node_config_fields(&self, type_id: &str) -> Option<Vec<NodeConfigField>> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some(node.config_fields())
    }

    /// Legacy compatibility flag. EventProducer has been removed, so this
    /// always returns false.
    pub fn is_event_producer(&self, _type_id: &str) -> bool {
        false
    }

    /// Get all registered node types
    pub fn get_all_types(&self) -> Vec<NodeTypeMetadata> {
        self.metadata.read().unwrap().values().cloned().collect()
    }

    /// Get node types by category
    pub fn get_types_by_category(&self, category: &str) -> Vec<NodeTypeMetadata> {
        self.metadata
            .read()
            .unwrap()
            .values()
            .filter(|meta| meta.category == category)
            .cloned()
            .collect()
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories: Vec<_> = self
            .metadata
            .read()
            .unwrap()
            .values()
            .map(|meta| meta.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }
}

/// Global singleton registry
pub static NODE_REGISTRY: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);

/// Helper macro to register a node type
#[macro_export]
macro_rules! register_node {
    ($type_id:expr, $display_name:expr, $category:expr, $description:expr, $node_struct:ty) => {
        $crate::graph::registry::NODE_REGISTRY
            .register(
                $type_id,
                $display_name,
                $category,
                $description,
                std::sync::Arc::new(|id: String, name: String| {
                    Box::new(<$node_struct>::new(id, name))
                }),
            )
            .unwrap();
    };
}

pub fn build_node_graph_from_definition(
    definition: &crate::graph::graph_io::NodeGraphDefinition,
) -> Result<crate::graph::NodeGraph> {
    let mut graph = crate::graph::NodeGraph::new();
    graph.set_definition(definition.clone());

    if !definition.edges.is_empty() {
        graph.set_edges(definition.edges.clone());
    }

    for node_def in &definition.nodes {
        let node = NODE_REGISTRY.create_node(
            &node_def.node_type,
            node_def.id.clone(),
            node_def.name.clone(),
        )?;

        // Parse inline values
        if !node_def.inline_values.is_empty() {
            let mut values = NodeConfigFlow::new();
            let ports: HashMap<String, DataType> = node
                .input_ports()
                .into_iter()
                .chain(node.output_ports().into_iter())
                .map(|p| (p.name, p.data_type))
                .chain(node.config_fields().into_iter().map(|field| (field.key, field.data_type)))
                .collect();

            for (port_name, json_val) in &node_def.inline_values {
                if let Some(data_type) = ports.get(port_name) {
                    if let Some(val) = json_to_data_value(json_val, data_type) {
                        values.insert(port_name.clone(), val);
                    }
                }
            }
            if !values.is_empty() {
                graph.inline_values.insert(node_def.id.clone(), values);
            }
        }

        graph.add_node(node)?;
    }

    let inline_values_snapshot = graph.inline_values.clone();
    for (node_id, node) in graph.nodes.iter_mut() {
        if let Some(inline_values) = inline_values_snapshot.get(node_id) {
            node.apply_inline_config(inline_values)?;
        }
    }

    // Second pass: nodes with dynamic input ports (e.g. FormatStringNode) only expose
    // their full port list after apply_inline_config. Re-collect any inline values that
    // were skipped in the first pass because the ports didn't exist yet.
    let extra_inline: Vec<(String, NodeConfigFlow)> = definition
        .nodes
        .iter()
        .filter_map(|node_def| {
            if node_def.inline_values.is_empty() {
                return None;
            }
            let node = graph.nodes.get(&node_def.id)?;
            let already_set: std::collections::HashSet<&str> = graph
                .inline_values
                .get(&node_def.id)
                .map(|m| m.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let ports: HashMap<String, DataType> = node
                .input_ports()
                .into_iter()
                .chain(node.output_ports().into_iter())
                .map(|p| (p.name, p.data_type))
                .chain(node.config_fields().into_iter().map(|field| (field.key, field.data_type)))
                .collect();
            let mut extra = NodeConfigFlow::new();
            for (port_name, json_val) in &node_def.inline_values {
                if !already_set.contains(port_name.as_str()) {
                    if let Some(data_type) = ports.get(port_name) {
                        if let Some(val) = json_to_data_value(json_val, data_type) {
                            extra.insert(port_name.clone(), val);
                        }
                    }
                }
            }
            if extra.is_empty() {
                None
            } else {
                Some((node_def.id.clone(), extra))
            }
        })
        .collect();
    for (node_id, extra_values) in extra_inline {
        graph.inline_values.entry(node_id).or_default().extend(extra_values);
    }

    let runtime_variable_store = graph.runtime_variable_store();
    graph.set_runtime_variable_store(runtime_variable_store);

    Ok(graph)
}

pub(crate) fn json_to_data_value(json: &Value, target_type: &DataType) -> Option<DataValue> {
    match (json, target_type) {
        (_, DataType::Any) => infer_any_data_value(json),
        (Value::String(s), DataType::String) => Some(DataValue::String(s.clone())),
        (Value::String(s), DataType::Password) => Some(DataValue::Password(s.clone())),
        (Value::String(s), DataType::Boolean) => {
            if s == "true" {
                Some(DataValue::Boolean(true))
            } else if s == "false" {
                Some(DataValue::Boolean(false))
            } else {
                None
            }
        }
        (Value::String(s), DataType::Integer) => s.parse().ok().map(DataValue::Integer),
        (Value::String(s), DataType::Float) => s.parse().ok().map(DataValue::Float),
        (Value::String(s), DataType::Json) => match serde_json::from_str(s) {
            Ok(v) => Some(DataValue::Json(v)),
            Err(_) => Some(DataValue::String(s.clone())), // Fallback? or Error? Or maybe just create Json string
        },

        (Value::Number(n), DataType::Integer) => n.as_i64().map(DataValue::Integer),
        (Value::Number(n), DataType::Float) => n.as_f64().map(DataValue::Float),

        (Value::Bool(b), DataType::Boolean) => Some(DataValue::Boolean(*b)),

        (v, DataType::Json) => Some(DataValue::Json(v.clone())),

        (Value::Array(items), DataType::Vector) => items
            .iter()
            .map(|item| match item {
                Value::Number(value) => value.as_f64().map(|v| v as f32),
                Value::String(value) => value.parse::<f32>().ok(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(DataValue::Vector),

        // Prefer the canonical serialized message so scripts preserve tool calls, usage and media.
        (Value::Object(map), DataType::LLMMessage) => {
            if let Ok(message) =
                serde_json::from_value::<crate::model_inference::llm::LLMMessage>(json.clone())
            {
                return Some(DataValue::LLMMessage(message));
            }

            // Compatibility with concise script input: {"role": "user", "content": "..."}.
            fn parse_role(v: &Value) -> crate::model_inference::llm::MessageRole {
                let s = v.as_str().unwrap_or("user").to_ascii_lowercase();
                match s.as_str() {
                    "system" => crate::model_inference::llm::MessageRole::System,
                    "assistant" => crate::model_inference::llm::MessageRole::Assistant,
                    "tool" => crate::model_inference::llm::MessageRole::Tool,
                    _ => crate::model_inference::llm::MessageRole::User,
                }
            }

            let role = map
                .get("role")
                .map(|v| parse_role(v))
                .unwrap_or(crate::model_inference::llm::MessageRole::User);
            let parts = match map.get("parts") {
                Some(Value::Array(parts)) => parts
                    .iter()
                    .filter_map(|part| {
                        serde_json::from_value::<crate::model_inference::llm::MessagePart>(
                            part.clone(),
                        )
                        .ok()
                    })
                    .collect(),
                Some(Value::Null) | None => map
                    .get("content")
                    .and_then(Value::as_str)
                    .map(|content| vec![crate::model_inference::llm::MessagePart::text(content)])
                    .unwrap_or_default(),
                Some(other) => serde_json::from_value::<crate::model_inference::llm::MessagePart>(
                    other.clone(),
                )
                .map(|part| vec![part])
                .unwrap_or_default(),
            };
            Some(DataValue::LLMMessage(crate::model_inference::llm::LLMMessage {
                role,
                parts,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
                usage: None,
            }))
        }

        (_, DataType::Sender) => serde_json::from_value::<
            crate::ims_bot_adapter::models::sender_model::Sender,
        >(json.clone())
        .ok()
        .map(DataValue::Sender),

        (_, DataType::MessageEvent) => serde_json::from_value::<
            crate::ims_bot_adapter::models::event_model::MessageEvent,
        >(json.clone())
        .ok()
        .map(DataValue::MessageEvent),

        // Single QQ Message from a JSON object: {"type": "text", "data": {"text": "..."}}
        (_, DataType::QQMessage) => {
            serde_json::from_value::<crate::ims_bot_adapter::models::message::Message>(json.clone())
                .ok()
                .map(DataValue::QQMessage)
        }

        (_, DataType::MessagePart) => {
            if let Ok(part) =
                serde_json::from_value::<crate::model_inference::llm::MessagePart>(json.clone())
            {
                return Some(DataValue::MessagePart(part));
            }
            let part_type = json.get("type").and_then(Value::as_str)?;
            let url = json
                .pointer("/media/original_source")
                .or_else(|| json.get("url"))
                .and_then(Value::as_str)?;
            match part_type {
                "image" => Some(DataValue::MessagePart(
                    crate::model_inference::llm::MessagePart::image_url_string(url),
                )),
                "video" => Some(DataValue::MessagePart(
                    crate::model_inference::llm::MessagePart::video_url_string(url),
                )),
                "text" => json
                    .get("text")
                    .and_then(Value::as_str)
                    .map(crate::model_inference::llm::MessagePart::text)
                    .map(DataValue::MessagePart),
                _ => None,
            }
        }

        (Value::Array(bytes), DataType::Binary) => bytes
            .iter()
            .map(|byte| byte.as_u64().filter(|byte| *byte <= u8::MAX as u64).map(|byte| byte as u8))
            .collect::<Option<Vec<_>>>()
            .map(DataValue::Binary),

        // Single Image payload from a JSON object.
        (_, DataType::Image) => {
            serde_json::from_value::<crate::graph::data_value::ImageData>(json.clone())
                .ok()
                .map(DataValue::Image)
        }

        // Generic Vec: recurse per element using the inner type.
        // Handles Vec<LLMMessage>, Vec<QQMessage>, and any other Vec<X>.
        (Value::Array(items), DataType::Vec(inner)) => {
            let parsed: Vec<DataValue> =
                items.iter().filter_map(|item| json_to_data_value(item, inner)).collect();
            Some(DataValue::Vec(inner.clone(), parsed))
        }

        _ => None,
    }
}

fn infer_any_data_value(json: &Value) -> Option<DataValue> {
    match json {
        Value::String(s) => Some(DataValue::String(s.clone())),
        Value::Number(n) => {
            n.as_i64().map(DataValue::Integer).or_else(|| n.as_f64().map(DataValue::Float))
        }
        Value::Bool(b) => Some(DataValue::Boolean(*b)),
        _ => Some(DataValue::Json(json.clone())),
    }
}

/// Register all node types that live within this crate.
/// Called by the main binary's `init_registry::init_node_registry` and also by
/// in-crate tests that need the registry populated.
pub fn init_node_registry() -> crate::error::Result<()> {
    use crate::graph::util::{
        FunctionInputsNode, FunctionNode, FunctionOutputsNode, GraphInputsNode, GraphOutputsNode,
    };

    register_node!(
        "function",
        "函数",
        "工具",
        "执行节点私有函数子图，输入输出端口由函数签名动态决定",
        FunctionNode
    );
    register_node!(
        "function_inputs",
        "函数输入",
        "内部",
        "函数子图内部边界节点，将调用参数展开为动态输出端口",
        FunctionInputsNode
    );
    register_node!(
        "function_outputs",
        "函数输出",
        "内部",
        "函数子图内部边界节点，汇总子图结果作为函数返回值",
        FunctionOutputsNode
    );
    register_node!(
        "graph_inputs",
        "节点图输入",
        "内部",
        "主节点图内部边界节点，将运行时参数展开为动态输出端口",
        GraphInputsNode
    );
    register_node!(
        "graph_outputs",
        "节点图输出",
        "内部",
        "主节点图内部边界节点，汇总主图结果作为返回值",
        GraphOutputsNode
    );
    Ok(())
}

pub fn init_node_registry_with_extensions(extra_registrars: &[RegistryInitFn]) -> Result<()> {
    init_node_registry()?;
    for init in extra_registrars {
        init()?;
    }
    let workspace_root = std::env::current_dir().map_err(|error| {
        crate::error::Error::ValidationError(format!("无法获取动态脚本运行时工作目录: {error}"))
    })?;
    let config = crate::config::ConfigCenter::shared()
        .load_root()
        .map(|root| root.node_runtime)
        .unwrap_or_default();
    crate::graph::script_node::register_script_catalog(&NODE_REGISTRY, &workspace_root, &config)?;
    Ok(())
}
