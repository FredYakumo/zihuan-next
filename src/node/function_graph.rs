use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::node::graph_io::{GraphPosition, GraphSize, NodeDefinition, NodeGraphDefinition};
use crate::node::{DataType, Port};

pub const FUNCTION_CONFIG_PORT: &str = "function_config";
pub const FUNCTION_SIGNATURE_PORT: &str = "signature";
pub const FUNCTION_RUNTIME_VALUES_PORT: &str = "runtime_values";

pub const FUNCTION_INPUTS_NODE_TYPE: &str = "function_inputs";
pub const FUNCTION_OUTPUTS_NODE_TYPE: &str = "function_outputs";

pub const FUNCTION_INPUTS_NODE_ID: &str = "__function_inputs__";
pub const FUNCTION_OUTPUTS_NODE_ID: &str = "__function_outputs__";

const DEFAULT_FUNCTION_WIDTH: f32 = 220.0;
const DEFAULT_FUNCTION_HEIGHT: f32 = 120.0;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionPortDef {
    pub name: String,
    pub data_type: DataType,
}

impl FunctionPortDef {
    pub fn to_port(&self, description: impl Into<String>) -> Port {
        Port::new(self.name.clone(), self.data_type.clone()).with_description(description)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmbeddedFunctionConfig {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: Vec<FunctionPortDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
    #[serde(default = "default_function_subgraph")]
    pub subgraph: NodeGraphDefinition,
}

pub fn default_function_subgraph() -> NodeGraphDefinition {
    let inputs: Vec<FunctionPortDef> = Vec::new();
    let outputs: Vec<FunctionPortDef> = Vec::new();

    NodeGraphDefinition {
        nodes: vec![
            build_function_inputs_node_definition(&inputs),
            build_function_outputs_node_definition(&outputs),
        ],
        edges: Vec::new(),
        hyperparameter_groups: Vec::new(),
        hyperparameters: Vec::new(),
        variables: Vec::new(),
        execution_results: HashMap::new(),
    }
}

pub fn default_embedded_function_config(name: impl Into<String>) -> EmbeddedFunctionConfig {
    EmbeddedFunctionConfig {
        name: name.into(),
        description: String::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
        subgraph: default_function_subgraph(),
    }
}

pub fn function_inputs_ports(signature: &[FunctionPortDef]) -> Vec<Port> {
    signature
        .iter()
        .map(|port| port.to_port(format!("函数输入 '{}'", port.name)))
        .collect()
}

pub fn function_outputs_ports(signature: &[FunctionPortDef]) -> Vec<Port> {
    signature
        .iter()
        .map(|port| port.to_port(format!("函数输出 '{}'", port.name)))
        .collect()
}

pub fn hidden_function_config_port() -> Port {
    Port::new(FUNCTION_CONFIG_PORT, DataType::Json)
        .with_description("隐藏的函数配置 JSON")
        .optional()
}

pub fn hidden_function_signature_port() -> Port {
    Port::new(FUNCTION_SIGNATURE_PORT, DataType::Json)
        .with_description("隐藏的函数签名 JSON")
        .optional()
}

pub fn hidden_function_runtime_values_port() -> Port {
    Port::new(FUNCTION_RUNTIME_VALUES_PORT, DataType::Json)
        .with_description("运行时注入的函数输入 JSON")
        .optional()
}

pub fn is_hidden_function_port(node_type: &str, port_name: &str) -> bool {
    match node_type {
        "function" => port_name == FUNCTION_CONFIG_PORT,
        FUNCTION_INPUTS_NODE_TYPE => {
            port_name == FUNCTION_SIGNATURE_PORT || port_name == FUNCTION_RUNTIME_VALUES_PORT
        }
        FUNCTION_OUTPUTS_NODE_TYPE => port_name == FUNCTION_SIGNATURE_PORT,
        _ => false,
    }
}

pub fn is_function_boundary_node(node_type: &str) -> bool {
    matches!(
        node_type,
        FUNCTION_INPUTS_NODE_TYPE | FUNCTION_OUTPUTS_NODE_TYPE
    )
}

pub fn embedded_function_config_from_node(node: &NodeDefinition) -> Option<EmbeddedFunctionConfig> {
    embedded_function_config_from_inline_values(&node.inline_values)
}

pub fn embedded_function_config_from_inline_values(
    inline_values: &HashMap<String, Value>,
) -> Option<EmbeddedFunctionConfig> {
    inline_values
        .get(FUNCTION_CONFIG_PORT)
        .and_then(embedded_function_config_from_value)
}

pub fn embedded_function_config_from_value(value: &Value) -> Option<EmbeddedFunctionConfig> {
    serde_json::from_value::<EmbeddedFunctionConfig>(value.clone()).ok()
}

pub fn function_signature_from_inline_values(
    inline_values: &HashMap<String, Value>,
) -> Option<Vec<FunctionPortDef>> {
    inline_values
        .get(FUNCTION_SIGNATURE_PORT)
        .and_then(function_signature_from_value)
}

pub fn function_signature_from_value(value: &Value) -> Option<Vec<FunctionPortDef>> {
    serde_json::from_value::<Vec<FunctionPortDef>>(value.clone()).ok()
}

pub fn sync_function_node_definition(
    node: &mut NodeDefinition,
    config: &EmbeddedFunctionConfig,
) -> bool {
    let mut changed = false;
    let mut normalized = config.clone();
    changed |= sync_function_subgraph(&mut normalized.subgraph);

    let description = if normalized.description.trim().is_empty() {
        None
    } else {
        Some(normalized.description.clone())
    };
    let input_ports = function_inputs_ports(&normalized.inputs);
    let output_ports = function_outputs_ports(&normalized.outputs);
    let config_json = serde_json::to_value(&normalized).unwrap_or(Value::Null);

    if node.name != normalized.name {
        node.name = normalized.name.clone();
        changed = true;
    }
    if node.description != description {
        node.description = description;
        changed = true;
    }
    if node.input_ports != input_ports {
        node.input_ports = input_ports;
        changed = true;
    }
    if node.output_ports != output_ports {
        node.output_ports = output_ports;
        changed = true;
    }
    if !node.dynamic_input_ports {
        node.dynamic_input_ports = true;
        changed = true;
    }
    if !node.dynamic_output_ports {
        node.dynamic_output_ports = true;
        changed = true;
    }
    if node
        .inline_values
        .get(FUNCTION_CONFIG_PORT)
        .map(|existing| existing != &config_json)
        .unwrap_or(true)
    {
        node.inline_values
            .insert(FUNCTION_CONFIG_PORT.to_string(), config_json);
        changed = true;
    }

    changed
}

pub fn sync_function_subgraph(subgraph: &mut NodeGraphDefinition) -> bool {
    let inputs_signature = subgraph
        .nodes
        .iter()
        .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
        .and_then(|node| function_signature_from_inline_values(&node.inline_values))
        .unwrap_or_default();
    let outputs_signature = subgraph
        .nodes
        .iter()
        .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
        .and_then(|node| function_signature_from_inline_values(&node.inline_values))
        .unwrap_or_default();

    sync_function_subgraph_signature(subgraph, &inputs_signature, &outputs_signature)
}

pub fn sync_function_subgraph_signature(
    subgraph: &mut NodeGraphDefinition,
    inputs: &[FunctionPortDef],
    outputs: &[FunctionPortDef],
) -> bool {
    let mut changed = false;

    changed |= upsert_boundary_node(
        subgraph,
        FUNCTION_INPUTS_NODE_ID,
        build_function_inputs_node_definition(inputs),
    );
    changed |= upsert_boundary_node(
        subgraph,
        FUNCTION_OUTPUTS_NODE_ID,
        build_function_outputs_node_definition(outputs),
    );

    let valid_inputs: HashMap<&str, Vec<&str>> = subgraph
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
    let valid_outputs: HashMap<&str, Vec<&str>> = subgraph
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

    let before_edges = subgraph.edges.len();
    subgraph.edges.retain(|edge| {
        valid_outputs
            .get(edge.from_node_id.as_str())
            .map(|ports| ports.contains(&edge.from_port.as_str()))
            .unwrap_or(false)
            && valid_inputs
                .get(edge.to_node_id.as_str())
                .map(|ports| ports.contains(&edge.to_port.as_str()))
                .unwrap_or(false)
    });
    changed |= before_edges != subgraph.edges.len();

    changed
}

fn upsert_boundary_node(
    subgraph: &mut NodeGraphDefinition,
    node_id: &str,
    replacement: NodeDefinition,
) -> bool {
    if let Some(existing) = subgraph.nodes.iter_mut().find(|node| node.id == node_id) {
        let position = existing.position.clone();
        let size = existing.size.clone();
        let mut replacement = replacement;
        // Always prefer the existing (user-dragged) position/size over the
        // hardcoded defaults supplied by build_function_*_node_definition.
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
            || existing.has_cycle != replacement.has_cycle;
        *existing = replacement;
        return changed;
    }

    subgraph.nodes.push(replacement);
    true
}

fn build_function_inputs_node_definition(signature: &[FunctionPortDef]) -> NodeDefinition {
    let mut inline_values = HashMap::new();
    inline_values.insert(
        FUNCTION_SIGNATURE_PORT.to_string(),
        serde_json::to_value(signature).unwrap_or(Value::Null),
    );

    NodeDefinition {
        id: FUNCTION_INPUTS_NODE_ID.to_string(),
        name: "函数输入".to_string(),
        description: Some("函数子图的输入边界节点".to_string()),
        node_type: FUNCTION_INPUTS_NODE_TYPE.to_string(),
        input_ports: vec![
            hidden_function_signature_port(),
            hidden_function_runtime_values_port(),
        ],
        output_ports: function_inputs_ports(signature),
        dynamic_input_ports: false,
        dynamic_output_ports: true,
        position: Some(GraphPosition { x: 60.0, y: 120.0 }),
        size: Some(GraphSize {
            width: DEFAULT_FUNCTION_WIDTH,
            height: DEFAULT_FUNCTION_HEIGHT,
        }),
        inline_values,
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
    }
}

fn build_function_outputs_node_definition(signature: &[FunctionPortDef]) -> NodeDefinition {
    let mut inline_values = HashMap::new();
    inline_values.insert(
        FUNCTION_SIGNATURE_PORT.to_string(),
        serde_json::to_value(signature).unwrap_or(Value::Null),
    );

    let mut input_ports = vec![hidden_function_signature_port()];
    input_ports.extend(function_outputs_ports(signature));

    NodeDefinition {
        id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
        name: "函数输出".to_string(),
        description: Some("函数子图的输出边界节点".to_string()),
        node_type: FUNCTION_OUTPUTS_NODE_TYPE.to_string(),
        input_ports,
        output_ports: Vec::new(),
        dynamic_input_ports: true,
        dynamic_output_ports: false,
        position: Some(GraphPosition { x: 520.0, y: 120.0 }),
        size: Some(GraphSize {
            width: DEFAULT_FUNCTION_WIDTH,
            height: DEFAULT_FUNCTION_HEIGHT,
        }),
        inline_values,
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
    }
}
