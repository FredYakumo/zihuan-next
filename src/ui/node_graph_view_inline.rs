use std::collections::HashMap;

use crate::error::Result;
use crate::node::graph_io::NodeGraphDefinition;
use crate::node::registry::NODE_REGISTRY;
use crate::ui::node_render::{inline_port_key, InlinePortValue};

pub(crate) fn build_inline_inputs_from_graph(graph: &NodeGraphDefinition) -> HashMap<String, InlinePortValue> {
    let mut map = HashMap::new();
    for node in &graph.nodes {
        for (port_name, val) in &node.inline_values {
            if node.port_bindings.contains_key(port_name) {
                continue;
            }
            let key = inline_port_key(&node.id, port_name);
            match val {
                serde_json::Value::String(s) => {
                    map.insert(key, InlinePortValue::Text(s.clone()));
                }
                serde_json::Value::Bool(b) => {
                    map.insert(key, InlinePortValue::Bool(*b));
                }
                serde_json::Value::Number(n) => {
                    map.insert(key, InlinePortValue::Text(n.to_string()));
                }
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    map.insert(key, InlinePortValue::Json(val.clone()));
                }
                _ => {}
            }
        }
    }
    map
}

pub(crate) fn apply_inline_inputs_to_graph(
    graph: &mut NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
) {
    for node in &mut graph.nodes {
        for port in &node.input_ports {
            if node.port_bindings.contains_key(&port.name) {
                continue;
            }
            let key = inline_port_key(&node.id, &port.name);
            if let Some(val) = inline_inputs.get(&key) {
                match val {
                    InlinePortValue::Text(s) => {
                        node.inline_values.insert(port.name.clone(), serde_json::Value::String(s.clone()));
                    }
                    InlinePortValue::Bool(b) => {
                        node.inline_values.insert(port.name.clone(), serde_json::Value::Bool(*b));
                    }
                    InlinePortValue::Json(v) => {
                        node.inline_values.insert(port.name.clone(), v.clone());
                    }
                }
            }
        }
    }
}

pub(crate) fn apply_hyperparameter_bindings_to_graph(
    graph: &mut NodeGraphDefinition,
    values: &std::collections::HashMap<String, serde_json::Value>,
) {
    for node in &mut graph.nodes {
        for (port_name, hp_name) in &node.port_bindings {
            if let Some(value) = values.get(hp_name.as_str()) {
                node.inline_values.insert(port_name.clone(), value.clone());
            }
        }
    }
}

fn message_list_key(node_id: &str) -> String {
    inline_port_key(node_id, "messages")
}

pub(crate) fn get_message_list_inline(
    inline_inputs: &HashMap<String, InlinePortValue>,
    node_id: &str,
) -> Vec<serde_json::Value> {
    let key = message_list_key(node_id);
    match inline_inputs.get(&key) {
        Some(InlinePortValue::Json(serde_json::Value::Array(items))) => items.clone(),
        _ => Vec::new(),
    }
}

pub(crate) fn set_message_list_inline(
    inline_inputs: &mut HashMap<String, InlinePortValue>,
    node_id: &str,
    items: Vec<serde_json::Value>,
) {
    let key = message_list_key(node_id);
    inline_inputs.insert(key, InlinePortValue::Json(serde_json::Value::Array(items)));
}

pub(crate) fn new_message_item(role: &str, content: &str) -> serde_json::Value {
    serde_json::json!({
        "role": role,
        "content": content,
    })
}

pub(crate) fn cycle_role(current: &str) -> &'static str {
    match current {
        "user" => "assistant",
        "assistant" => "system",
        "system" => "tool",
        _ => "user",
    }
}

pub(crate) fn add_node_to_graph(graph: &mut NodeGraphDefinition, type_id: &str) -> Result<()> {
    let id = next_node_id(graph);

    let all_types = NODE_REGISTRY.get_all_types();
    let metadata = all_types.iter().find(|meta| meta.type_id == type_id);

    let display_name = metadata
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| "NewNode".to_string());

    let dummy_node = NODE_REGISTRY.create_node(type_id, &id, &display_name)?;

    graph.nodes.push(crate::node::graph_io::NodeDefinition {
        id,
        name: display_name,
        description: dummy_node.description().map(|s| s.to_string()),
        node_type: type_id.to_string(),
        input_ports: dummy_node.input_ports(),
        output_ports: dummy_node.output_ports(),
        dynamic_input_ports: dummy_node.has_dynamic_input_ports(),
        dynamic_output_ports: dummy_node.has_dynamic_output_ports(),
        position: None,
        size: None,
        inline_values: HashMap::new(),
        port_bindings: HashMap::new(),
        has_error: false,
    });

    Ok(())
}

fn next_node_id(graph: &NodeGraphDefinition) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("node_{index}");
        if !graph.nodes.iter().any(|node| node.id == candidate) {
            return candidate;
        }
        index += 1;
    }
}
