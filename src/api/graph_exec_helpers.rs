use std::collections::HashMap;

use serde_json::Value;
use zihuan_node::graph_io::{NodeGraphDefinition, PortBindingKind};
use zihuan_node::function_graph::{
    embedded_function_config_from_node, sync_function_node_definition, FUNCTION_CONFIG_PORT,
};
use zihuan_llm::brain_tool::BrainToolDefinition;

/// Apply hyperparameter values to a graph definition by expanding all PORT_BINDING entries
/// that reference hyperparameters.  This matches the logic previously in
/// `src/ui/node_graph_view_inline.rs::apply_hyperparameter_bindings_to_graph`.
pub fn apply_hyperparameter_bindings(
    graph: &mut NodeGraphDefinition,
    values: &HashMap<String, Value>,
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
            apply_hyperparameter_bindings(&mut config.subgraph, values);
            if let Ok(value) = serde_json::to_value(&config) {
                node.inline_values
                    .insert(FUNCTION_CONFIG_PORT.to_string(), value);
            }
        }

        if let Some(tools_value) = node.inline_values.get("tools_config").cloned() {
            if let Ok(mut tools) =
                serde_json::from_value::<Vec<BrainToolDefinition>>(tools_value)
            {
                for tool in &mut tools {
                    apply_hyperparameter_bindings(&mut tool.subgraph, values);
                }
                if let Ok(value) = serde_json::to_value(&tools) {
                    node.inline_values.insert("tools_config".to_string(), value);
                }
            }
        }
    }
}
