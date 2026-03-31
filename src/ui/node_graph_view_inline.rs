use std::collections::HashMap;

use crate::error::Result;
use crate::llm::brain_tool::BrainToolDefinition;
use crate::node::function_graph::{
    default_embedded_function_config, embedded_function_config_from_node,
    sync_function_node_definition, FUNCTION_CONFIG_PORT, FUNCTION_INPUTS_NODE_TYPE,
    FUNCTION_OUTPUTS_NODE_TYPE,
};
use crate::node::graph_io::{NodeGraphDefinition, PortBindingKind};
use crate::node::registry::NODE_REGISTRY;
use crate::ui::node_render::{inline_port_key, InlinePortValue};

pub(crate) fn build_inline_inputs_from_graph(
    graph: &NodeGraphDefinition,
) -> HashMap<String, InlinePortValue> {
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
                        node.inline_values
                            .insert(port.name.clone(), serde_json::Value::String(s.clone()));
                    }
                    InlinePortValue::Bool(b) => {
                        node.inline_values
                            .insert(port.name.clone(), serde_json::Value::Bool(*b));
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
        for (port_name, binding) in &node.port_bindings {
            if binding.kind != PortBindingKind::Hyperparameter {
                continue;
            }
            if let Some(value) = values.get(binding.name.as_str()) {
                node.inline_values.insert(port_name.clone(), value.clone());
            }
        }

        if let Some(mut config) = embedded_function_config_from_node(node) {
            apply_hyperparameter_bindings_to_graph(&mut config.subgraph, values);
            if let Ok(value) = serde_json::to_value(&config) {
                node.inline_values
                    .insert(FUNCTION_CONFIG_PORT.to_string(), value);
            }
        }

        if let Some(tools_value) = node.inline_values.get("tools_config").cloned() {
            if let Ok(mut tools) = serde_json::from_value::<Vec<BrainToolDefinition>>(tools_value) {
                for tool in &mut tools {
                    apply_hyperparameter_bindings_to_graph(&mut tool.subgraph, values);
                }
                if let Ok(value) = serde_json::to_value(&tools) {
                    node.inline_values.insert("tools_config".to_string(), value);
                }
            }
        }
    }
}

pub(crate) fn materialize_graph_for_execution(
    graph: &mut NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
    hyperparameter_values: &HashMap<String, serde_json::Value>,
) {
    // Inline JSON configs can contain embedded subgraph payloads like function_config/tools_config.
    // Apply them first, then inject hyperparameter-bound values so nested bindings are preserved.
    apply_inline_inputs_to_graph(graph, inline_inputs);
    apply_hyperparameter_bindings_to_graph(graph, hyperparameter_values);
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
    if matches!(type_id, FUNCTION_INPUTS_NODE_TYPE | FUNCTION_OUTPUTS_NODE_TYPE) {
        return Err(crate::error::Error::ValidationError(
            "函数边界节点不能从节点面板直接添加".to_string(),
        ));
    }

    let id = next_node_id(graph);

    let all_types = NODE_REGISTRY.get_all_types();
    let metadata = all_types.iter().find(|meta| meta.type_id == type_id);

    let display_name = metadata
        .map(|m| m.display_name.clone())
        .unwrap_or_else(|| "NewNode".to_string());

    let dummy_node = NODE_REGISTRY.create_node(type_id, &id, &display_name)?;

    let mut node_definition = crate::node::graph_io::NodeDefinition {
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
        has_cycle: false,
    };

    if type_id == "function" {
        let config = default_embedded_function_config(node_definition.name.clone());
        sync_function_node_definition(&mut node_definition, &config);
    }

    graph.nodes.push(node_definition);

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::{
        apply_hyperparameter_bindings_to_graph, build_inline_inputs_from_graph,
        materialize_graph_for_execution,
    };
    use crate::llm::brain_tool::{BrainToolDefinition, ToolParamDef};
    use crate::node::function_graph::{
        default_embedded_function_config, sync_function_node_definition, FunctionPortDef,
    };
    use crate::node::graph_io::{NodeDefinition, PortBinding};
    use crate::node::DataType;

    fn binding_node(id: &str, port_name: &str, hp_name: &str) -> NodeDefinition {
        NodeDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            node_type: "string_data".to_string(),
            input_ports: vec![crate::node::Port::new(port_name.to_string(), DataType::String)],
            output_ports: Vec::new(),
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::from([(
                port_name.to_string(),
                PortBinding::hyperparameter(hp_name.to_string()),
            )]),
            has_error: false,
            has_cycle: false,
        }
    }

    #[test]
    fn apply_hyperparameter_bindings_recurses_into_function_subgraphs() {
        let mut function_node = NodeDefinition {
            id: "fn_1".to_string(),
            name: "fn_1".to_string(),
            description: None,
            node_type: "function".to_string(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            dynamic_input_ports: true,
            dynamic_output_ports: true,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        };
        let mut config = default_embedded_function_config("demo");
        config.inputs = vec![FunctionPortDef {
            name: "name".to_string(),
            data_type: DataType::String,
        }];
        config.subgraph.nodes.push(binding_node("inner_1", "text", "hp_name"));
        sync_function_node_definition(&mut function_node, &config);

        let mut graph = crate::node::graph_io::NodeGraphDefinition {
            nodes: vec![function_node],
            ..Default::default()
        };
        let values = HashMap::from([("hp_name".to_string(), json!("alice"))]);

        apply_hyperparameter_bindings_to_graph(&mut graph, &values);

        let config = crate::node::function_graph::embedded_function_config_from_node(&graph.nodes[0])
            .expect("function config");
        let inner = config
            .subgraph
            .nodes
            .iter()
            .find(|node| node.id == "inner_1")
            .expect("inner node");
        assert_eq!(inner.inline_values.get("text"), Some(&json!("alice")));
    }

    #[test]
    fn apply_hyperparameter_bindings_recurses_into_brain_tool_subgraphs() {
        let mut brain_node = NodeDefinition {
            id: "brain_1".to_string(),
            name: "brain_1".to_string(),
            description: None,
            node_type: "brain".to_string(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        };
        let mut tool = BrainToolDefinition {
            id: "tool_1".to_string(),
            name: "tool_1".to_string(),
            description: String::new(),
            parameters: vec![ToolParamDef {
                name: "arg".to_string(),
                data_type: DataType::String,
                desc: String::new(),
            }],
            outputs: Vec::new(),
            subgraph: crate::node::function_graph::default_function_subgraph(),
        };
        tool.subgraph
            .nodes
            .push(binding_node("inner_tool_1", "text", "hp_name"));
        brain_node.inline_values.insert(
            "tools_config".to_string(),
            serde_json::to_value(vec![tool]).expect("serialize tools"),
        );

        let mut graph = crate::node::graph_io::NodeGraphDefinition {
            nodes: vec![brain_node],
            ..Default::default()
        };
        let values = HashMap::from([("hp_name".to_string(), json!("alice"))]);

        apply_hyperparameter_bindings_to_graph(&mut graph, &values);

        let tools = graph.nodes[0]
            .inline_values
            .get("tools_config")
            .and_then(|value| serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone()).ok())
            .expect("tools config");
        let inner = tools[0]
            .subgraph
            .nodes
            .iter()
            .find(|node| node.id == "inner_tool_1")
            .expect("inner tool node");
        assert_eq!(inner.inline_values.get("text"), Some(&json!("alice")));
    }

    #[test]
    fn materialize_graph_for_execution_preserves_function_subgraph_bindings() {
        let mut function_node = NodeDefinition {
            id: "fn_1".to_string(),
            name: "fn_1".to_string(),
            description: None,
            node_type: "function".to_string(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            dynamic_input_ports: true,
            dynamic_output_ports: true,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        };
        let mut config = default_embedded_function_config("demo");
        config.inputs = vec![FunctionPortDef {
            name: "name".to_string(),
            data_type: DataType::String,
        }];
        config.subgraph.nodes.push(binding_node("inner_1", "text", "hp_name"));
        sync_function_node_definition(&mut function_node, &config);

        let mut graph = crate::node::graph_io::NodeGraphDefinition {
            nodes: vec![function_node],
            ..Default::default()
        };
        let inline_inputs = build_inline_inputs_from_graph(&graph);
        let values = HashMap::from([("hp_name".to_string(), json!("alice"))]);

        materialize_graph_for_execution(&mut graph, &inline_inputs, &values);

        let config = crate::node::function_graph::embedded_function_config_from_node(&graph.nodes[0])
            .expect("function config");
        let inner = config
            .subgraph
            .nodes
            .iter()
            .find(|node| node.id == "inner_1")
            .expect("inner node");
        assert_eq!(inner.inline_values.get("text"), Some(&json!("alice")));
    }

    #[test]
    fn materialize_graph_for_execution_preserves_brain_tool_subgraph_bindings() {
        let mut brain_node = NodeDefinition {
            id: "brain_1".to_string(),
            name: "brain_1".to_string(),
            description: None,
            node_type: "brain".to_string(),
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        };
        let mut tool = BrainToolDefinition {
            id: "tool_1".to_string(),
            name: "tool_1".to_string(),
            description: String::new(),
            parameters: vec![ToolParamDef {
                name: "arg".to_string(),
                data_type: DataType::String,
                desc: String::new(),
            }],
            outputs: Vec::new(),
            subgraph: crate::node::function_graph::default_function_subgraph(),
        };
        tool.subgraph
            .nodes
            .push(binding_node("inner_tool_1", "text", "hp_name"));
        brain_node.inline_values.insert(
            "tools_config".to_string(),
            serde_json::to_value(vec![tool]).expect("serialize tools"),
        );

        let mut graph = crate::node::graph_io::NodeGraphDefinition {
            nodes: vec![brain_node],
            ..Default::default()
        };
        let inline_inputs = build_inline_inputs_from_graph(&graph);
        let values = HashMap::from([("hp_name".to_string(), json!("alice"))]);

        materialize_graph_for_execution(&mut graph, &inline_inputs, &values);

        let tools = graph.nodes[0]
            .inline_values
            .get("tools_config")
            .and_then(|value| serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone()).ok())
            .expect("tools config");
        let inner = tools[0]
            .subgraph
            .nodes
            .iter()
            .find(|node| node.id == "inner_tool_1")
            .expect("inner tool node");
        assert_eq!(inner.inline_values.get("text"), Some(&json!("alice")));
    }
}
