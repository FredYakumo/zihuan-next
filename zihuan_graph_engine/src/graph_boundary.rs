use std::collections::HashMap;

use serde_json::Value;

use crate::function_graph::{
    function_inputs_ports, function_outputs_ports, function_signature_from_inline_values,
    hidden_function_runtime_values_port, hidden_function_signature_port, FunctionPortDef,
    FUNCTION_SIGNATURE_PORT,
};
use crate::graph_io::{GraphPosition, GraphSize, NodeDefinition, NodeGraphDefinition};
use crate::Port;

pub const GRAPH_INPUTS_NODE_TYPE: &str = "graph_inputs";
pub const GRAPH_OUTPUTS_NODE_TYPE: &str = "graph_outputs";

pub const GRAPH_INPUTS_NODE_ID: &str = "__graph_inputs__";
pub const GRAPH_OUTPUTS_NODE_ID: &str = "__graph_outputs__";

const DEFAULT_BOUNDARY_WIDTH: f32 = 220.0;
const DEFAULT_BOUNDARY_HEIGHT: f32 = 120.0;

pub fn default_root_graph_definition() -> NodeGraphDefinition {
    let mut graph = NodeGraphDefinition::default();
    sync_root_graph_io_signature(&mut graph, &[], &[]);
    graph
}

pub fn graph_inputs_ports(signature: &[FunctionPortDef]) -> Vec<Port> {
    function_inputs_ports(signature)
        .into_iter()
        .map(|port| {
            let name = port.name.clone();
            port.with_description(format!("节点图输入 '{}'", name))
        })
        .collect()
}

pub fn graph_outputs_ports(signature: &[FunctionPortDef]) -> Vec<Port> {
    function_outputs_ports(signature)
        .into_iter()
        .map(|port| {
            let name = port.name.clone();
            port.with_description(format!("节点图输出 '{}'", name))
        })
        .collect()
}

pub fn sync_root_graph_io(graph: &mut NodeGraphDefinition) -> bool {
    // Prefer top-level graph_inputs/graph_outputs when present so an explicit graph
    // signature can repair stale boundary-node inline signatures from older saves.
    let inputs_signature = if !graph.graph_inputs.is_empty() {
        graph.graph_inputs.clone()
    } else {
        graph
            .nodes
            .iter()
            .find(|node| node.id == GRAPH_INPUTS_NODE_ID)
            .and_then(|node| function_signature_from_inline_values(&node.inline_values))
            .unwrap_or_default()
    };
    let outputs_signature = if !graph.graph_outputs.is_empty() {
        graph.graph_outputs.clone()
    } else {
        graph
            .nodes
            .iter()
            .find(|node| node.id == GRAPH_OUTPUTS_NODE_ID)
            .and_then(|node| function_signature_from_inline_values(&node.inline_values))
            .unwrap_or_default()
    };

    sync_root_graph_io_signature(graph, &inputs_signature, &outputs_signature)
}

pub fn sync_root_graph_io_signature(
    graph: &mut NodeGraphDefinition,
    inputs: &[FunctionPortDef],
    outputs: &[FunctionPortDef],
) -> bool {
    let mut changed = false;

    if graph.graph_inputs != inputs {
        graph.graph_inputs = inputs.to_vec();
        changed = true;
    }
    if graph.graph_outputs != outputs {
        graph.graph_outputs = outputs.to_vec();
        changed = true;
    }

    changed |= upsert_boundary_node(
        graph,
        GRAPH_INPUTS_NODE_ID,
        build_graph_inputs_node_definition(inputs),
    );
    changed |= upsert_boundary_node(
        graph,
        GRAPH_OUTPUTS_NODE_ID,
        build_graph_outputs_node_definition(outputs),
    );

    let valid_inputs: HashMap<&str, Vec<&str>> = graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.input_ports
                    .iter()
                    .map(|port| port.name.as_str())
                    .collect(),
            )
        })
        .collect();
    let valid_outputs: HashMap<&str, Vec<&str>> = graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.output_ports
                    .iter()
                    .map(|port| port.name.as_str())
                    .collect(),
            )
        })
        .collect();

    let before_edges = graph.edges.len();
    graph.edges.retain(|edge| {
        valid_outputs
            .get(edge.from_node_id.as_str())
            .map(|ports| ports.contains(&edge.from_port.as_str()))
            .unwrap_or(false)
            && valid_inputs
                .get(edge.to_node_id.as_str())
                .map(|ports| ports.contains(&edge.to_port.as_str()))
                .unwrap_or(false)
    });
    changed |= before_edges != graph.edges.len();

    changed
}

pub fn root_graph_to_tool_subgraph(graph: &NodeGraphDefinition) -> NodeGraphDefinition {
    let mut subgraph = graph.clone();
    let input_signature = subgraph.graph_inputs.clone();
    let output_signature = subgraph.graph_outputs.clone();

    for node in &mut subgraph.nodes {
        if node.id == GRAPH_INPUTS_NODE_ID {
            node.id = crate::function_graph::FUNCTION_INPUTS_NODE_ID.to_string();
            node.name = "函数输入".to_string();
            node.node_type = crate::function_graph::FUNCTION_INPUTS_NODE_TYPE.to_string();
        } else if node.id == GRAPH_OUTPUTS_NODE_ID {
            node.id = crate::function_graph::FUNCTION_OUTPUTS_NODE_ID.to_string();
            node.name = "函数输出".to_string();
            node.node_type = crate::function_graph::FUNCTION_OUTPUTS_NODE_TYPE.to_string();
        }
    }

    for edge in &mut subgraph.edges {
        if edge.from_node_id == GRAPH_INPUTS_NODE_ID {
            edge.from_node_id = crate::function_graph::FUNCTION_INPUTS_NODE_ID.to_string();
        }
        if edge.to_node_id == GRAPH_OUTPUTS_NODE_ID {
            edge.to_node_id = crate::function_graph::FUNCTION_OUTPUTS_NODE_ID.to_string();
        }
    }

    subgraph.graph_inputs.clear();
    subgraph.graph_outputs.clear();
    crate::function_graph::sync_function_subgraph_signature(
        &mut subgraph,
        &input_signature,
        &output_signature,
    );
    subgraph
}

fn upsert_boundary_node(
    graph: &mut NodeGraphDefinition,
    node_id: &str,
    replacement: NodeDefinition,
) -> bool {
    if let Some(existing) = graph.nodes.iter_mut().find(|node| node.id == node_id) {
        let position = existing.position.clone();
        let size = existing.size.clone();
        let mut replacement = replacement;
        replacement.position = position.or(replacement.position);
        replacement.size = size.or(replacement.size);
        let changed = existing.id != replacement.id
            || existing.name != replacement.name
            || existing.description != replacement.description
            || existing.node_type != replacement.node_type
            || existing.input_ports != replacement.input_ports
            || existing.output_ports != replacement.output_ports
            || existing.dynamic_input_ports != replacement.dynamic_input_ports
            || existing.dynamic_output_ports != replacement.dynamic_output_ports
            || existing.position.as_ref().map(|p| (p.x, p.y))
                != replacement.position.as_ref().map(|p| (p.x, p.y))
            || existing.size.as_ref().map(|s| (s.width, s.height))
                != replacement.size.as_ref().map(|s| (s.width, s.height))
            || existing.inline_values != replacement.inline_values
            || existing.port_bindings != replacement.port_bindings
            || existing.has_error != replacement.has_error
            || existing.has_cycle != replacement.has_cycle
            || existing.disabled != replacement.disabled;
        *existing = replacement;
        return changed;
    }

    graph.nodes.push(replacement);
    true
}

fn build_graph_inputs_node_definition(signature: &[FunctionPortDef]) -> NodeDefinition {
    let mut inline_values = HashMap::new();
    inline_values.insert(
        FUNCTION_SIGNATURE_PORT.to_string(),
        serde_json::to_value(signature).unwrap_or(Value::Null),
    );

    NodeDefinition {
        id: GRAPH_INPUTS_NODE_ID.to_string(),
        name: "节点图输入".to_string(),
        description: Some("主节点图的输入边界节点".to_string()),
        node_type: GRAPH_INPUTS_NODE_TYPE.to_string(),
        input_ports: vec![
            hidden_function_signature_port(),
            hidden_function_runtime_values_port(),
        ],
        output_ports: graph_inputs_ports(signature),
        dynamic_input_ports: false,
        dynamic_output_ports: true,
        position: Some(GraphPosition { x: 60.0, y: 120.0 }),
        size: Some(GraphSize {
            width: DEFAULT_BOUNDARY_WIDTH,
            height: DEFAULT_BOUNDARY_HEIGHT,
        }),
        inline_values,
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
        disabled: false,
    }
}

fn build_graph_outputs_node_definition(signature: &[FunctionPortDef]) -> NodeDefinition {
    let mut inline_values = HashMap::new();
    inline_values.insert(
        FUNCTION_SIGNATURE_PORT.to_string(),
        serde_json::to_value(signature).unwrap_or(Value::Null),
    );

    let mut input_ports = vec![hidden_function_signature_port()];
    input_ports.extend(graph_outputs_ports(signature));

    NodeDefinition {
        id: GRAPH_OUTPUTS_NODE_ID.to_string(),
        name: "节点图输出".to_string(),
        description: Some("主节点图的输出边界节点".to_string()),
        node_type: GRAPH_OUTPUTS_NODE_TYPE.to_string(),
        input_ports,
        output_ports: Vec::new(),
        dynamic_input_ports: true,
        dynamic_output_ports: false,
        position: Some(GraphPosition { x: 520.0, y: 120.0 }),
        size: Some(GraphSize {
            width: DEFAULT_BOUNDARY_WIDTH,
            height: DEFAULT_BOUNDARY_HEIGHT,
        }),
        inline_values,
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
        disabled: false,
    }
}
