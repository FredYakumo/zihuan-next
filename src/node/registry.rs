use crate::error::Result;
use crate::node::{DataType, DataValue, Node};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Node factory function type
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;

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

        self.factories
            .write()
            .unwrap()
            .insert(type_id.clone(), factory);
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
    /// Returns `None` if the type is not registered.
    pub fn get_node_ports(
        &self,
        type_id: &str,
    ) -> Option<(Vec<crate::node::Port>, Vec<crate::node::Port>)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((node.input_ports(), node.output_ports()))
    }

    pub fn get_node_dynamic_port_flags(&self, type_id: &str) -> Option<(bool, bool)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((
            node.has_dynamic_input_ports(),
            node.has_dynamic_output_ports(),
        ))
    }

    /// Returns true if the registered node type is an EventProducer.
    pub fn is_event_producer(&self, type_id: &str) -> bool {
        let factories = self.factories.read().unwrap();
        let Some(factory) = factories.get(type_id) else {
            return false;
        };
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        node.node_type() == crate::node::NodeType::EventProducer
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
        $crate::node::registry::NODE_REGISTRY
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

/// Build a NodeGraph from a NodeGraphDefinition
pub fn build_node_graph_from_definition(
    definition: &crate::node::graph_io::NodeGraphDefinition,
) -> Result<crate::node::NodeGraph> {
    let mut graph = crate::node::NodeGraph::new();
    graph.set_definition(definition.clone());

    if !definition.edges.is_empty() {
        graph.set_edges(definition.edges.clone());
    }

    // Create all nodes
    for node_def in &definition.nodes {
        let node = NODE_REGISTRY.create_node(
            &node_def.node_type,
            node_def.id.clone(),
            node_def.name.clone(),
        )?;

        // Parse inline values
        if !node_def.inline_values.is_empty() {
            let mut values = HashMap::new();
            let ports: HashMap<String, DataType> = node
                .input_ports()
                .into_iter()
                .map(|p| (p.name, p.data_type))
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
    let extra_inline: Vec<(String, HashMap<String, DataValue>)> = definition
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
                .map(|p| (p.name, p.data_type))
                .collect();
            let mut extra = HashMap::new();
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
        graph
            .inline_values
            .entry(node_id)
            .or_default()
            .extend(extra_values);
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

        // Single OpenAIMessage from a JSON object: {"role": "user", "content": "..."}
        (Value::Object(map), DataType::OpenAIMessage) => {
            fn parse_role(v: &Value) -> zihuan_llm::MessageRole {
                let s = v.as_str().unwrap_or("user").to_ascii_lowercase();
                match s.as_str() {
                    "system" => zihuan_llm::MessageRole::System,
                    "assistant" => zihuan_llm::MessageRole::Assistant,
                    "tool" => zihuan_llm::MessageRole::Tool,
                    _ => zihuan_llm::MessageRole::User,
                }
            }

            let role = map
                .get("role")
                .map(|v| parse_role(v))
                .unwrap_or(zihuan_llm::MessageRole::User);
            let content = match map.get("content") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(other) => Some(other.to_string()),
            };
            Some(DataValue::OpenAIMessage(zihuan_llm::OpenAIMessage {
                role,
                content,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
            }))
        }

        // Single QQ Message from a JSON object: {"type": "text", "data": {"text": "..."}}
        (_, DataType::QQMessage) => {
            serde_json::from_value::<crate::bot_adapter::models::message::Message>(json.clone())
                .ok()
                .map(DataValue::QQMessage)
        }

        // Generic Vec: recurse per element using the inner type.
        // Handles Vec<OpenAIMessage>, Vec<QQMessage>, and any other Vec<X>.
        (Value::Array(items), DataType::Vec(inner)) => {
            let parsed: Vec<DataValue> = items
                .iter()
                .filter_map(|item| json_to_data_value(item, inner))
                .collect();
            Some(DataValue::Vec(inner.clone(), parsed))
        }

        _ => None,
    }
}

fn infer_any_data_value(json: &Value) -> Option<DataValue> {
    match json {
        Value::String(s) => Some(DataValue::String(s.clone())),
        Value::Number(n) => n
            .as_i64()
            .map(DataValue::Integer)
            .or_else(|| n.as_f64().map(DataValue::Float)),
        Value::Bool(b) => Some(DataValue::Boolean(*b)),
        _ => Some(DataValue::Json(json.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::json_to_data_value;
    use crate::node::{DataType, DataValue};

    #[test]
    fn parse_message_list_inline_value() {
        let json = serde_json::json!([
            {"role": "user", "content": "hi"},
            {"role": "ASSISTANT", "content": "hello"},
            {"role": "weird", "content": null}
        ]);

        let val = json_to_data_value(&json, &DataType::Vec(Box::new(DataType::OpenAIMessage)))
            .expect("should parse Vec<OpenAIMessage>");

        match val {
            DataValue::Vec(_, list) => {
                assert_eq!(list.len(), 3);
                match &list[0] {
                    DataValue::OpenAIMessage(m) => {
                        assert_eq!(zihuan_llm::role_to_str(&m.role), "user");
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
                match &list[1] {
                    DataValue::OpenAIMessage(m) => {
                        assert_eq!(zihuan_llm::role_to_str(&m.role), "assistant");
                        assert_eq!(m.content.as_deref(), Some("hello"));
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
                match &list[2] {
                    DataValue::OpenAIMessage(m) => {
                        // Unknown role falls back to user
                        assert_eq!(zihuan_llm::role_to_str(&m.role), "user");
                        assert_eq!(m.content, None);
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
            }
            _ => panic!("unexpected DataValue variant"),
        }
    }

    #[test]
    fn parse_any_inline_value() {
        let val = json_to_data_value(&serde_json::json!(123), &DataType::Any)
            .expect("should parse Any integer");

        match val {
            DataValue::Integer(value) => assert_eq!(value, 123),
            _ => panic!("unexpected DataValue variant"),
        }
    }
}
