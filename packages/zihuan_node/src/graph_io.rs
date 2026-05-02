use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use crate::brain_tool_spec::{
    brain_shared_inputs_from_value, brain_tool_input_signature, is_tool_subgraph_owner,
    normalized_tool_outputs_for_owner, BrainToolDefinition, BRAIN_SHARED_INPUTS_PORT,
    BRAIN_TOOLS_CONFIG_PORT,
};
use crate::data_value::DataType;
use crate::function_graph::{
    default_embedded_function_config, embedded_function_config_from_node,
    sync_function_node_definition, sync_function_subgraph_signature,
};
use crate::{DataValue, Node, NodeGraph, Port};
use zihuan_core::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortBindingKind {
    Hyperparameter,
    Variable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortBinding {
    pub kind: PortBindingKind,
    pub name: String,
}

impl PortBinding {
    pub fn hyperparameter(name: impl Into<String>) -> Self {
        Self {
            kind: PortBindingKind::Hyperparameter,
            name: name.into(),
        }
    }

    pub fn variable(name: impl Into<String>) -> Self {
        Self {
            kind: PortBindingKind::Variable,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphVariable {
    pub name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub initial_value: Option<Value>,
}

/// Graph-level metadata: human-readable name, description, and semver version.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphMetadata {
    /// Human-readable graph name (may differ from the filename).
    #[serde(default)]
    pub name: Option<String>,
    /// Free-text description of what this graph does.
    #[serde(default)]
    pub description: Option<String>,
    /// Semver-style version string, e.g. "1.0.0".
    #[serde(default)]
    pub version: Option<String>,
}

/// A graph-level hyperparameter (variable) that can be bound to node input ports.
/// Values are NOT stored here – they live in a separate per-graph YAML file
/// in the central app-data directory (`hyperparam_store`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperParameter {
    /// Unique name within the graph
    pub name: String,
    /// Must be one of: String, Integer, Float, Boolean, Password
    pub data_type: DataType,
    /// Logical group for shared hyperparameter storage.
    #[serde(default = "default_hyperparameter_group")]
    pub group: String,
    /// Whether execution is blocked when this hyperparameter has no value
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}

fn default_hyperparameter_group() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeGraphDefinition {
    pub nodes: Vec<NodeDefinition>,
    pub edges: Vec<EdgeDefinition>,
    #[serde(default)]
    pub hyperparameter_groups: Vec<String>,
    #[serde(default)]
    pub hyperparameters: Vec<HyperParameter>,
    #[serde(default)]
    pub variables: Vec<GraphVariable>,
    #[serde(default)]
    pub metadata: GraphMetadata,
    #[serde(skip)]
    pub execution_results: HashMap<String, HashMap<String, DataValue>>,
}

#[derive(Debug, Clone)]
pub struct LoadedGraphDefinition {
    pub graph: NodeGraphDefinition,
    pub migrated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_type: String,
    pub input_ports: Vec<Port>,
    pub output_ports: Vec<Port>,
    #[serde(default)]
    pub dynamic_input_ports: bool,
    #[serde(default)]
    pub dynamic_output_ports: bool,
    pub position: Option<GraphPosition>,
    pub size: Option<GraphSize>,
    #[serde(default)]
    pub inline_values: HashMap<String, Value>,
    #[serde(default, deserialize_with = "deserialize_port_bindings")]
    pub port_bindings: HashMap<String, PortBinding>,
    #[serde(default)]
    pub has_error: bool,
    #[serde(default)]
    pub has_cycle: bool,
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PortBindingDef {
    Legacy(String),
    Structured(PortBinding),
}

fn deserialize_port_bindings<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, PortBinding>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let bindings = HashMap::<String, PortBindingDef>::deserialize(deserializer)?;
    Ok(bindings
        .into_iter()
        .map(|(port_name, binding)| {
            let binding = match binding {
                PortBindingDef::Legacy(name) => PortBinding::hyperparameter(name),
                PortBindingDef::Structured(binding) => binding,
            };
            (port_name, binding)
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeDefinition {
    pub from_node_id: String,
    pub from_port: String,
    pub to_node_id: String,
    pub to_port: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSize {
    pub width: f32,
    pub height: f32,
}

pub type CycleEdgeKey = (String, String, String, String);

pub fn load_graph_definition_from_json(path: impl AsRef<Path>) -> Result<NodeGraphDefinition> {
    Ok(load_graph_definition_from_json_with_migration(path)?.graph)
}

pub fn load_graph_definition_from_json_with_migration(
    path: impl AsRef<Path>,
) -> Result<LoadedGraphDefinition> {
    let content = fs::read_to_string(path.as_ref())?;
    // Backward-compat: replace removed type names before parsing so old saved graphs load cleanly.
    // refresh_port_types() will then overwrite these with the live registry types.
    let content = content
        .replace(
            "\"data_type\": \"MessageList\"",
            "\"data_type\": {\"Vec\":\"OpenAIMessage\"}",
        )
        .replace(
            "\"data_type\":\"MessageList\"",
            "\"data_type\":{\"Vec\":\"OpenAIMessage\"}",
        )
        .replace(
            "\"data_type\": \"QQMessageList\"",
            "\"data_type\": {\"Vec\":\"QQMessage\"}",
        )
        .replace(
            "\"data_type\":\"QQMessageList\"",
            "\"data_type\":{\"Vec\":\"QQMessage\"}",
        )
        // Also migrate old "List" variant name (renamed to "Vec")
        .replace("\"data_type\": {\"List\":", "\"data_type\": {\"Vec\":")
        .replace("\"data_type\":{\"List\":", "\"data_type\":{\"Vec\":");
    let mut graph: NodeGraphDefinition = serde_json::from_str(&content)?;
    let before_refresh = serde_json::to_value(&graph).ok();
    refresh_port_types(&mut graph);
    let migrated = before_refresh
        .and_then(|before| {
            serde_json::to_value(&graph)
                .ok()
                .map(|after| before != after)
        })
        .unwrap_or(false);
    Ok(LoadedGraphDefinition { graph, migrated })
}

/// Refresh port `data_type` fields in a loaded graph by looking up the canonical types from
/// the node registry. This migrates graphs saved with stale port types (e.g. `String` instead
/// of `Password`) without requiring a manual file edit.
pub fn refresh_port_types(graph: &mut NodeGraphDefinition) {
    refresh_port_types_internal(graph);
}

fn refresh_port_types_internal(graph: &mut NodeGraphDefinition) {
    use crate::registry::NODE_REGISTRY;
    for node in &mut graph.nodes {
        if let Some((canonical_inputs, canonical_outputs)) =
            NODE_REGISTRY.get_node_ports(&node.node_type)
        {
            if let Some((dynamic_inputs, dynamic_outputs)) =
                NODE_REGISTRY.get_node_dynamic_port_flags(&node.node_type)
            {
                node.dynamic_input_ports = dynamic_inputs;
                node.dynamic_output_ports = dynamic_outputs;
            }

            if !node.dynamic_input_ports {
                node.input_ports.retain(|port| {
                    canonical_inputs
                        .iter()
                        .any(|canonical| canonical.name == port.name)
                });
            }
            for canon in &canonical_inputs {
                if !node.input_ports.iter().any(|p| p.name == canon.name) {
                    node.input_ports.push(canon.clone());
                }
            }
            for port in &mut node.input_ports {
                if let Some(canonical) = canonical_inputs.iter().find(|p| p.name == port.name) {
                    port.data_type = canonical.data_type.clone();
                }
            }

            if !node.dynamic_output_ports {
                node.output_ports.retain(|port| {
                    canonical_outputs
                        .iter()
                        .any(|canonical| canonical.name == port.name)
                });
            }
            for canon in &canonical_outputs {
                if !node.output_ports.iter().any(|p| p.name == canon.name) {
                    node.output_ports.push(canon.clone());
                }
            }
            for port in &mut node.output_ports {
                if let Some(canonical) = canonical_outputs.iter().find(|p| p.name == port.name) {
                    port.data_type = canonical.data_type.clone();
                }
            }

            if !node.dynamic_input_ports {
                let all_port_names = canonical_inputs
                    .iter()
                    .chain(canonical_outputs.iter())
                    .map(|port| port.name.as_str())
                    .collect::<Vec<_>>();
                node.inline_values
                    .retain(|key, _| all_port_names.contains(&key.as_str()));
            }
        }
    }

    rebuild_dynamic_ports_from_inline_values(graph);
    fix_function_node_input_types_from_edges(graph);
    refresh_embedded_subgraphs(graph);
    prune_invalid_edges(graph);
}

fn rebuild_dynamic_ports_from_inline_values(graph: &mut NodeGraphDefinition) {
    use crate::registry::{json_to_data_value, NODE_REGISTRY};

    for node in &mut graph.nodes {
        if !node.dynamic_input_ports && !node.dynamic_output_ports {
            continue;
        }

        let Ok(mut runtime_node) =
            NODE_REGISTRY.create_node(&node.node_type, node.id.clone(), node.name.clone())
        else {
            continue;
        };

        let input_types: HashMap<String, DataType> = runtime_node
            .input_ports()
            .into_iter()
            .map(|port| (port.name, port.data_type))
            .collect();

        let inline_values: HashMap<String, DataValue> = node
            .inline_values
            .iter()
            .filter_map(|(port_name, json_val)| {
                input_types
                    .get(port_name)
                    .and_then(|data_type| json_to_data_value(json_val, data_type))
                    .map(|value| (port_name.clone(), value))
            })
            .collect();

        if runtime_node.apply_inline_config(&inline_values).is_err() {
            node.has_error = true;
            continue;
        }

        if node.dynamic_input_ports {
            node.input_ports = runtime_node.input_ports();
        }
        if node.dynamic_output_ports {
            node.output_ports = runtime_node.output_ports();
        }
    }
}

/// 根据外层图中连接到 function 节点输入端口的边，修正 function_config.inputs 中
/// 保存了错误类型的条目（如旧版转换时将 BotAdapterRef/SessionStateRef 写成了 String）。
/// 这是对旧 JSON 的加载迁移：以边另一端的源端口（已经过注册表刷新）为准，覆盖config里的错误类型。
fn fix_function_node_input_types_from_edges(graph: &mut NodeGraphDefinition) {
    use crate::function_graph::{
        embedded_function_config_from_node, sync_function_node_definition,
    };

    // 构建 node_id → output_ports 映射（端口类型已经过注册表刷新）
    let output_port_map: HashMap<String, Vec<Port>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.output_ports.clone()))
        .collect();

    // 收集每个 function 节点应被修正的 (port_name → canonical DataType)
    let mut corrections: HashMap<String, Vec<(String, DataType)>> = HashMap::new();
    for edge in &graph.edges {
        let Some(to_node) = graph.nodes.iter().find(|n| n.id == edge.to_node_id) else {
            continue;
        };
        if to_node.node_type != "function" {
            continue;
        }
        let Some(from_ports) = output_port_map.get(&edge.from_node_id) else {
            continue;
        };
        let Some(from_port) = from_ports.iter().find(|p| p.name == edge.from_port) else {
            continue;
        };
        corrections
            .entry(edge.to_node_id.clone())
            .or_default()
            .push((edge.to_port.clone(), from_port.data_type.clone()));
    }

    if corrections.is_empty() {
        return;
    }

    for node in &mut graph.nodes {
        if node.node_type != "function" {
            continue;
        }
        let Some(fixes) = corrections.get(&node.id) else {
            continue;
        };
        let Some(mut config) = embedded_function_config_from_node(node) else {
            continue;
        };
        let mut changed = false;
        for (port_name, correct_type) in fixes {
            if let Some(port_def) = config.inputs.iter_mut().find(|p| &p.name == port_name) {
                if &port_def.data_type != correct_type {
                    log::debug!(
                        "[graph_io] 迁移 function 节点 '{}' 输入端口 '{}' 类型: {} → {}",
                        node.name,
                        port_name,
                        port_def.data_type,
                        correct_type
                    );
                    port_def.data_type = correct_type.clone();
                    changed = true;
                }
            }
        }
        if changed {
            sync_function_node_definition(node, &config);
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Validation & Auto-Fix
// ─────────────────────────────────────────────────────────────

/// A single compatibility issue found when validating a graph definition
/// against the current node registry.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// `"error"` or `"warning"`
    pub severity: String,
    pub message: String,
}

impl ValidationIssue {
    fn error(msg: impl Into<String>) -> Self {
        Self {
            severity: "error".into(),
            message: msg.into(),
        }
    }
    fn warning(msg: impl Into<String>) -> Self {
        Self {
            severity: "warning".into(),
            message: msg.into(),
        }
    }
}

/// Validate a loaded `NodeGraphDefinition` against the live node registry.
/// Returns a (possibly empty) list of issues. Does NOT mutate the definition.
pub fn validate_graph_definition(graph: &NodeGraphDefinition) -> Vec<ValidationIssue> {
    let mut issues = validate_graph_definition_local(graph);
    issues.extend(validate_embedded_subgraphs(graph));
    issues
}

fn validate_graph_definition_local(graph: &NodeGraphDefinition) -> Vec<ValidationIssue> {
    use crate::registry::NODE_REGISTRY;
    let mut issues = Vec::new();

    // Build a quick lookup: node_id → NodeDefinition
    let node_map: HashMap<String, &NodeDefinition> =
        graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    for node in &graph.nodes {
        match NODE_REGISTRY.get_node_ports(&node.node_type) {
            None => {
                issues.push(ValidationIssue::error(format!(
                    "节点 \"{}\" 的类型 \"{}\" 在注册表中不存在",
                    node.name, node.node_type
                )));
            }
            Some((canonical_inputs, canonical_outputs)) => {
                // Check for REQUIRED ports in registry but missing from JSON (inputs)
                if !node.dynamic_input_ports {
                    for canon_port in &canonical_inputs {
                        if canon_port.required
                            && !node.input_ports.iter().any(|p| p.name == canon_port.name)
                        {
                            issues.push(ValidationIssue::error(format!(
                                "节点 \"{}\" 缺少必要输入端口 \"{}\"",
                                node.name, canon_port.name
                            )));
                        }
                    }
                }
                // Check for ports in JSON but absent from registry (inputs)
                if !node.dynamic_input_ports {
                    for port in &node.input_ports {
                        if !canonical_inputs.iter().any(|p| p.name == port.name) {
                            issues.push(ValidationIssue::warning(format!(
                                "节点 \"{}\" 存在已删除的输入端口 \"{}\"",
                                node.name, port.name
                            )));
                        }
                    }
                }
                // Check for REQUIRED ports in registry but missing from JSON (outputs)
                if !node.dynamic_output_ports {
                    for canon_port in &canonical_outputs {
                        if canon_port.required
                            && !node.output_ports.iter().any(|p| p.name == canon_port.name)
                        {
                            issues.push(ValidationIssue::error(format!(
                                "节点 \"{}\" 缺少必要输出端口 \"{}\"",
                                node.name, canon_port.name
                            )));
                        }
                    }
                }
                // Check for ports in JSON but absent from registry (outputs)
                if !node.dynamic_output_ports {
                    for port in &node.output_ports {
                        if !canonical_outputs.iter().any(|p| p.name == port.name) {
                            issues.push(ValidationIssue::warning(format!(
                                "节点 \"{}\" 存在已删除的输出端口 \"{}\"",
                                node.name, port.name
                            )));
                        }
                    }
                }
                // Check inline_values keys against all known port names
                if !node.dynamic_input_ports {
                    let all_port_names: Vec<&str> = canonical_inputs
                        .iter()
                        .chain(canonical_outputs.iter())
                        .map(|p| p.name.as_str())
                        .collect();
                    for key in node.inline_values.keys() {
                        if !all_port_names.contains(&key.as_str()) {
                            issues.push(ValidationIssue::warning(format!(
                                "节点 \"{}\" 的内联值 \"{}\" 对应的端口不存在",
                                node.name, key
                            )));
                        }
                    }
                }
            }
        }
    }

    // Validate edges: node IDs and port names must exist
    for edge in &graph.edges {
        let from_ok = node_map
            .get(&edge.from_node_id)
            .map(|n| n.output_ports.iter().any(|p| p.name == edge.from_port))
            .unwrap_or(false);
        if !from_ok {
            issues.push(ValidationIssue::error(format!(
                "无效连接：源节点 \"{}\" 的输出端口 \"{}\" 不存在",
                edge.from_node_id, edge.from_port
            )));
        }
        let to_ok = node_map
            .get(&edge.to_node_id)
            .map(|n| n.input_ports.iter().any(|p| p.name == edge.to_port))
            .unwrap_or(false);
        if !to_ok {
            issues.push(ValidationIssue::error(format!(
                "无效连接：目标节点 \"{}\" 的输入端口 \"{}\" 不存在",
                edge.to_node_id, edge.to_port
            )));
        }
    }

    issues
}

/// Apply automatic in-memory fixes to make the graph consistent with the
/// current registry. Does NOT write anything to disk.
///
/// Fix strategy:
/// - Unregistered node type → remove node and all connected edges
/// - Missing ports vs registry → add the canonical port definition
/// - Extra ports not in registry → remove them; also drop any edges and inline_values referencing them
/// - Invalid edges (bad node/port reference) → remove
/// - Orphan inline_values (no matching port) → remove
pub fn auto_fix_graph_definition(graph: &mut NodeGraphDefinition) {
    auto_fix_graph_definition_local(graph);
    auto_fix_embedded_subgraphs(graph);
}

fn auto_fix_graph_definition_local(graph: &mut NodeGraphDefinition) {
    use crate::registry::NODE_REGISTRY;

    // Phase 0: collect and remove nodes with unknown types, along with their edges.
    let mut unknown_node_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for node in &graph.nodes {
        if NODE_REGISTRY.get_node_ports(&node.node_type).is_none() {
            log::info!(
                "[auto_fix] Removing node '{}' with unknown type '{}' (id={})",
                node.name,
                node.node_type,
                node.id
            );
            unknown_node_ids.insert(node.id.clone());
        }
    }
    if !unknown_node_ids.is_empty() {
        graph.nodes.retain(|n| !unknown_node_ids.contains(&n.id));
        graph.edges.retain(|e| {
            !unknown_node_ids.contains(&e.from_node_id) && !unknown_node_ids.contains(&e.to_node_id)
        });
    }

    // Track (node_id, port_name) of ports that no longer exist after fix –
    // used to prune dangling edges.
    let mut removed_output_ports: Vec<(String, String)> = Vec::new();
    let mut removed_input_ports: Vec<(String, String)> = Vec::new();

    for node in &mut graph.nodes {
        match NODE_REGISTRY.get_node_ports(&node.node_type) {
            None => {
                // Should not happen — unknown nodes were removed in Phase 0.
                node.has_error = true;
            }
            Some((canonical_inputs, canonical_outputs)) => {
                node.has_error = false;
                if let Some((dynamic_inputs, dynamic_outputs)) =
                    NODE_REGISTRY.get_node_dynamic_port_flags(&node.node_type)
                {
                    node.dynamic_input_ports = dynamic_inputs;
                    node.dynamic_output_ports = dynamic_outputs;
                }

                // Remove input ports not present in registry
                if !node.dynamic_input_ports {
                    let before: Vec<String> =
                        node.input_ports.iter().map(|p| p.name.clone()).collect();
                    node.input_ports
                        .retain(|p| canonical_inputs.iter().any(|c| c.name == p.name));
                    for removed in &before {
                        if !node.input_ports.iter().any(|p| &p.name == removed) {
                            removed_input_ports.push((node.id.clone(), removed.clone()));
                        }
                    }
                    // Add input ports missing from JSON
                    for canon in &canonical_inputs {
                        if !node.input_ports.iter().any(|p| p.name == canon.name) {
                            node.input_ports.push(canon.clone());
                        }
                    }
                }

                // Remove output ports not present in registry
                if !node.dynamic_output_ports {
                    let before: Vec<String> =
                        node.output_ports.iter().map(|p| p.name.clone()).collect();
                    node.output_ports
                        .retain(|p| canonical_outputs.iter().any(|c| c.name == p.name));
                    for removed in &before {
                        if !node.output_ports.iter().any(|p| &p.name == removed) {
                            removed_output_ports.push((node.id.clone(), removed.clone()));
                        }
                    }
                    // Add output ports missing from JSON
                    for canon in &canonical_outputs {
                        if !node.output_ports.iter().any(|p| p.name == canon.name) {
                            node.output_ports.push(canon.clone());
                        }
                    }
                }

                // Remove orphan inline_values (no matching port in registry)
                if !node.dynamic_input_ports {
                    let all_canonical_names: Vec<&str> = canonical_inputs
                        .iter()
                        .chain(canonical_outputs.iter())
                        .map(|p| p.name.as_str())
                        .collect();
                    node.inline_values
                        .retain(|k, _| all_canonical_names.contains(&k.as_str()));
                }
            }
        }
    }

    // Build set of valid (node_id, output_port) and (node_id, input_port) for edge validation
    let node_map: HashMap<String, &NodeDefinition> =
        graph.nodes.iter().map(|n| (n.id.clone(), n)).collect();

    graph.edges.retain(|edge| {
        // Drop if referencing a port we just removed
        if removed_output_ports
            .iter()
            .any(|(nid, port)| nid == &edge.from_node_id && port == &edge.from_port)
        {
            return false;
        }
        if removed_input_ports
            .iter()
            .any(|(nid, port)| nid == &edge.to_node_id && port == &edge.to_port)
        {
            return false;
        }
        // Drop if node or port referenced doesn't exist at all
        let from_ok = node_map
            .get(&edge.from_node_id)
            .map(|n| n.output_ports.iter().any(|p| p.name == edge.from_port))
            .unwrap_or(false);
        let to_ok = node_map
            .get(&edge.to_node_id)
            .map(|n| n.input_ports.iter().any(|p| p.name == edge.to_port))
            .unwrap_or(false);
        from_ok && to_ok
    });

    rebuild_dynamic_ports_from_inline_values(graph);
    prune_invalid_edges(graph);
}

fn prune_invalid_edges(graph: &mut NodeGraphDefinition) {
    let node_map: HashMap<&str, (&[Port], &[Port])> = graph
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                (node.input_ports.as_slice(), node.output_ports.as_slice()),
            )
        })
        .collect();

    graph.edges.retain(|edge| {
        let from_ok = node_map
            .get(edge.from_node_id.as_str())
            .map(|(_, outputs)| outputs.iter().any(|port| port.name == edge.from_port))
            .unwrap_or(false);
        let to_ok = node_map
            .get(edge.to_node_id.as_str())
            .map(|(inputs, _)| inputs.iter().any(|port| port.name == edge.to_port))
            .unwrap_or(false);
        from_ok && to_ok
    });
}

fn refresh_embedded_subgraphs(graph: &mut NodeGraphDefinition) {
    for node in &mut graph.nodes {
        if node.node_type == "function" {
            let mut config = embedded_function_config_from_node(node)
                .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));
            refresh_port_types_internal(&mut config.subgraph);
            sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);
            sync_function_node_definition(node, &config);
            continue;
        }

        if !is_tool_subgraph_owner(&node.node_type) {
            continue;
        }

        let shared_inputs = node
            .inline_values
            .get(BRAIN_SHARED_INPUTS_PORT)
            .and_then(brain_shared_inputs_from_value)
            .unwrap_or_default();

        let Some(value) = node.inline_values.get(BRAIN_TOOLS_CONFIG_PORT).cloned() else {
            continue;
        };
        let Ok(mut tools) = serde_json::from_value::<Vec<BrainToolDefinition>>(value) else {
            continue;
        };

        for (index, tool) in tools.iter_mut().enumerate() {
            tool.ensure_defaults(index + 1);
            refresh_port_types_internal(&mut tool.subgraph);
            let input_signature = brain_tool_input_signature(&shared_inputs, tool);
            let outputs = normalized_tool_outputs_for_owner(&node.node_type, tool);
            sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &outputs);
        }

        if let Ok(value) = serde_json::to_value(&tools) {
            node.inline_values
                .insert(BRAIN_TOOLS_CONFIG_PORT.to_string(), value);
        }
    }
}

fn validate_embedded_subgraphs(graph: &NodeGraphDefinition) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for node in &graph.nodes {
        if node.node_type == "function" {
            match embedded_function_config_from_node(node) {
                Some(config) => {
                    let prefix = format!("函数节点 \"{}\" 的子图", node.name);
                    issues.extend(
                        validate_graph_definition(&config.subgraph)
                            .into_iter()
                            .map(|issue| prefixed_issue(prefix.clone(), issue)),
                    );
                }
                None => {}
            }
        }

        if !is_tool_subgraph_owner(&node.node_type) {
            continue;
        }

        if let Some(value) = node.inline_values.get(BRAIN_TOOLS_CONFIG_PORT) {
            match serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone()) {
                Ok(tools) => {
                    for tool in tools {
                        let prefix = format!(
                            "{} 节点 \"{}\" 的 Tool \"{}\" 子图",
                            node.node_type, node.name, tool.name
                        );
                        issues.extend(
                            validate_graph_definition(&tool.subgraph)
                                .into_iter()
                                .map(|issue| prefixed_issue(prefix.clone(), issue)),
                        );
                    }
                }
                Err(error) => issues.push(ValidationIssue::error(format!(
                    "{} 节点 \"{}\" 的 tools_config 无法解析: {}",
                    node.node_type, node.name, error
                ))),
            }
        }
    }

    issues
}

fn auto_fix_embedded_subgraphs(graph: &mut NodeGraphDefinition) {
    for node in &mut graph.nodes {
        if node.node_type == "function" {
            let mut config = embedded_function_config_from_node(node)
                .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));
            auto_fix_graph_definition(&mut config.subgraph);
            sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);
            sync_function_node_definition(node, &config);
            continue;
        }

        if !is_tool_subgraph_owner(&node.node_type) {
            continue;
        }

        let shared_inputs = node
            .inline_values
            .get(BRAIN_SHARED_INPUTS_PORT)
            .and_then(brain_shared_inputs_from_value)
            .unwrap_or_default();

        let Some(value) = node.inline_values.get(BRAIN_TOOLS_CONFIG_PORT).cloned() else {
            continue;
        };
        let Ok(mut tools) = serde_json::from_value::<Vec<BrainToolDefinition>>(value) else {
            continue;
        };

        for (index, tool) in tools.iter_mut().enumerate() {
            tool.ensure_defaults(index + 1);
            auto_fix_graph_definition(&mut tool.subgraph);
            let input_signature = brain_tool_input_signature(&shared_inputs, tool);
            let outputs = normalized_tool_outputs_for_owner(&node.node_type, tool);
            sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &outputs);
        }

        if let Ok(value) = serde_json::to_value(&tools) {
            node.inline_values
                .insert(BRAIN_TOOLS_CONFIG_PORT.to_string(), value);
        }
    }
}

fn prefixed_issue(prefix: String, issue: ValidationIssue) -> ValidationIssue {
    ValidationIssue {
        severity: issue.severity,
        message: format!("{}: {}", prefix, issue.message),
    }
}

pub fn save_graph_definition_to_json(
    path: impl AsRef<Path>,
    graph: &NodeGraphDefinition,
) -> Result<()> {
    let content = serde_json::to_string_pretty(graph)?;
    fs::write(path.as_ref(), content)?;
    Ok(())
}

pub fn ensure_positions(graph: &mut NodeGraphDefinition) {
    let spacing_x = 220.0;
    let spacing_y = 140.0;
    let cols = 4usize;

    for (index, node) in graph.nodes.iter_mut().enumerate() {
        if node.position.is_none() {
            let col = (index % cols) as f32;
            let row = (index / cols) as f32;
            node.position = Some(GraphPosition {
                x: 40.0 + col * spacing_x,
                y: 40.0 + row * spacing_y,
            });
        }
    }
}

fn collect_cycle_members(graph: &NodeGraphDefinition) -> (HashSet<String>, HashSet<CycleEdgeKey>) {
    struct TarjanState {
        next_index: usize,
        index_by_node: HashMap<String, usize>,
        lowlink_by_node: HashMap<String, usize>,
        stack: Vec<String>,
        on_stack: HashSet<String>,
        components: Vec<Vec<String>>,
    }

    fn strong_connect(
        node_id: &str,
        adjacency: &HashMap<String, Vec<String>>,
        state: &mut TarjanState,
    ) {
        let node_id_string = node_id.to_string();
        state
            .index_by_node
            .insert(node_id_string.clone(), state.next_index);
        state
            .lowlink_by_node
            .insert(node_id_string.clone(), state.next_index);
        state.next_index += 1;
        state.stack.push(node_id_string.clone());
        state.on_stack.insert(node_id_string.clone());

        if let Some(neighbors) = adjacency.get(node_id) {
            for neighbor in neighbors {
                if !state.index_by_node.contains_key(neighbor) {
                    strong_connect(neighbor, adjacency, state);
                    let neighbor_lowlink =
                        *state.lowlink_by_node.get(neighbor).unwrap_or(&usize::MAX);
                    if let Some(lowlink) = state.lowlink_by_node.get_mut(&node_id_string) {
                        *lowlink = (*lowlink).min(neighbor_lowlink);
                    }
                } else if state.on_stack.contains(neighbor) {
                    let neighbor_index = *state.index_by_node.get(neighbor).unwrap_or(&usize::MAX);
                    if let Some(lowlink) = state.lowlink_by_node.get_mut(&node_id_string) {
                        *lowlink = (*lowlink).min(neighbor_index);
                    }
                }
            }
        }

        let node_index = *state
            .index_by_node
            .get(&node_id_string)
            .unwrap_or(&usize::MAX);
        let node_lowlink = *state
            .lowlink_by_node
            .get(&node_id_string)
            .unwrap_or(&usize::MAX);
        if node_index == node_lowlink {
            let mut component = Vec::new();
            while let Some(current) = state.stack.pop() {
                state.on_stack.remove(&current);
                component.push(current.clone());
                if current == node_id_string {
                    break;
                }
            }
            state.components.push(component);
        }
    }

    let mut adjacency: HashMap<String, Vec<String>> = graph
        .nodes
        .iter()
        .map(|node| (node.id.clone(), Vec::new()))
        .collect();
    let mut self_loops = HashSet::new();
    for edge in &graph.edges {
        adjacency
            .entry(edge.from_node_id.clone())
            .or_default()
            .push(edge.to_node_id.clone());
        if edge.from_node_id == edge.to_node_id {
            self_loops.insert(edge.from_node_id.clone());
        }
    }

    let mut state = TarjanState {
        next_index: 0,
        index_by_node: HashMap::new(),
        lowlink_by_node: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        components: Vec::new(),
    };

    for node in &graph.nodes {
        if !state.index_by_node.contains_key(&node.id) {
            strong_connect(&node.id, &adjacency, &mut state);
        }
    }

    let mut cycle_node_ids = HashSet::new();
    let mut node_component_index = HashMap::new();
    let mut cyclic_components = HashSet::new();

    for (component_index, component) in state.components.iter().enumerate() {
        let is_cycle_component = component.len() > 1
            || component
                .first()
                .map(|node_id| self_loops.contains(node_id))
                .unwrap_or(false);
        for node_id in component {
            node_component_index.insert(node_id.clone(), component_index);
            if is_cycle_component {
                cycle_node_ids.insert(node_id.clone());
            }
        }
        if is_cycle_component {
            cyclic_components.insert(component_index);
        }
    }

    let cycle_edge_keys = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_component = node_component_index.get(&edge.from_node_id)?;
            let to_component = node_component_index.get(&edge.to_node_id)?;
            if from_component == to_component && cyclic_components.contains(from_component) {
                Some((
                    edge.from_node_id.clone(),
                    edge.from_port.clone(),
                    edge.to_node_id.clone(),
                    edge.to_port.clone(),
                ))
            } else {
                None
            }
        })
        .collect();

    (cycle_node_ids, cycle_edge_keys)
}

pub fn find_cycle_node_ids(graph: &NodeGraphDefinition) -> HashSet<String> {
    collect_cycle_members(graph).0
}

pub fn find_cycle_edge_keys(graph: &NodeGraphDefinition) -> HashSet<CycleEdgeKey> {
    collect_cycle_members(graph).1
}

/// Compute node height from port count, matching the geometry module logic.
fn calc_node_height(node: &NodeDefinition) -> f32 {
    const GRID: f32 = 20.0;
    let port_rows = node.input_ports.len().max(node.output_ports.len()) as f32;
    let default_min = GRID * (3.0f32.max(2.0 + port_rows) + 0.8);
    let min_h = match node.node_type.as_str() {
        "message_list_data" | "qq_message_list_data" => default_min.max(GRID * 8.0),
        "brain" => default_min.max(GRID * 6.2),
        _ => default_min,
    };
    node.size.as_ref().map_or(min_h, |s| s.height.max(min_h))
}

/// Compute node width, matching the geometry module logic.
fn calc_node_width(node: &NodeDefinition) -> f32 {
    const MIN_WIDTH: f32 = 200.0;
    node.size
        .as_ref()
        .map_or(MIN_WIDTH, |s| s.width.max(MIN_WIDTH))
}

/// Auto-layout all nodes in a hierarchical left-to-right arrangement following data flow.
/// Roots are placed at the leftmost column; each additional level moves one column right.
/// Multiple root chains are stacked vertically. Chains longer than MAX_COLS wrap to a new band below.
/// Node sizes are taken into account for spacing.
pub fn auto_layout(graph: &mut NodeGraphDefinition) {
    const ORIGIN_X: f32 = 40.0;
    const ORIGIN_Y: f32 = 40.0;
    const H_GAP: f32 = 60.0; // horizontal gap between columns
    const V_GAP: f32 = 20.0; // vertical gap between nodes in the same column
    const BAND_GAP: f32 = 60.0; // extra vertical gap between wrap bands
    const MAX_COLS: usize = 8;

    if graph.nodes.is_empty() {
        return;
    }

    // Index node dimensions by id
    let node_dims: HashMap<String, (f32, f32)> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), (calc_node_width(n), calc_node_height(n))))
        .collect();

    // Build successor and predecessor maps
    let mut successors: HashMap<String, Vec<String>> = HashMap::new();
    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    for node in &graph.nodes {
        successors.entry(node.id.clone()).or_default();
        predecessors.entry(node.id.clone()).or_default();
    }
    for edge in &graph.edges {
        successors
            .entry(edge.from_node_id.clone())
            .or_default()
            .push(edge.to_node_id.clone());
        predecessors
            .entry(edge.to_node_id.clone())
            .or_default()
            .push(edge.from_node_id.clone());
    }

    // Compute topological level: level[n] = max(level[preds]) + 1, roots = 0
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    for node in &graph.nodes {
        in_degree.insert(node.id.clone(), predecessors[&node.id].len());
    }
    let mut level: HashMap<String, usize> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for node in &graph.nodes {
        if in_degree[&node.id] == 0 {
            level.insert(node.id.clone(), 0);
            queue.push_back(node.id.clone());
        }
    }
    let mut remaining_in: HashMap<String, usize> = in_degree.clone();
    while let Some(id) = queue.pop_front() {
        let cur_level = level[&id];
        let succs = successors[&id].clone();
        for succ in succs {
            let new_level = cur_level + 1;
            let entry = level.entry(succ.clone()).or_insert(0);
            if new_level > *entry {
                *entry = new_level;
            }
            let deg = remaining_in.entry(succ.clone()).or_insert(1);
            *deg = deg.saturating_sub(1);
            if *deg == 0 {
                queue.push_back(succ);
            }
        }
    }
    for node in &graph.nodes {
        level.entry(node.id.clone()).or_insert(0);
    }

    // Find roots ordered by their position in graph.nodes
    let roots: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| in_degree[&n.id] == 0)
        .map(|n| n.id.clone())
        .collect();

    // Assign track (root group) via BFS from each root in order
    let mut track: HashMap<String, usize> = HashMap::new();
    let mut discovery_order: HashMap<String, usize> = HashMap::new();
    let mut order_counter = 0usize;
    for (root_idx, root_id) in roots.iter().enumerate() {
        let mut bfs: VecDeque<String> = VecDeque::new();
        if !track.contains_key(root_id) {
            track.insert(root_id.clone(), root_idx);
            discovery_order.insert(root_id.clone(), order_counter);
            order_counter += 1;
            bfs.push_back(root_id.clone());
        }
        while let Some(id) = bfs.pop_front() {
            let succs = successors[&id].clone();
            for succ in succs {
                if !track.contains_key(&succ) {
                    track.insert(succ.clone(), root_idx);
                    discovery_order.insert(succ.clone(), order_counter);
                    order_counter += 1;
                    bfs.push_back(succ);
                }
            }
        }
    }
    let fallback_track = roots.len();
    for node in &graph.nodes {
        track.entry(node.id.clone()).or_insert(fallback_track);
        discovery_order
            .entry(node.id.clone())
            .or_insert(order_counter);
        order_counter += 1;
    }

    // Group nodes by (band, col_in_band), sorted by (track, discovery_order)
    let mut col_groups: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for node in &graph.nodes {
        let lv = level[&node.id];
        col_groups
            .entry((lv / MAX_COLS, lv % MAX_COLS))
            .or_default()
            .push(node.id.clone());
    }
    for nodes_in_col in col_groups.values_mut() {
        nodes_in_col.sort_by_key(|id| (track[id], discovery_order[id]));
    }

    let max_band = col_groups.keys().map(|(b, _)| *b).max().unwrap_or(0);

    // For each (band, col): compute max node width, and total column height (sum of heights + gaps)
    // col_x[band][col] = ORIGIN_X + sum of (max_width[band][0..col] + H_GAP)
    // col_total_height[band][col] = sum of node heights + (n-1)*V_GAP
    let mut col_max_width: HashMap<(usize, usize), f32> = HashMap::new();
    let mut col_total_height: HashMap<(usize, usize), f32> = HashMap::new();
    for (&key, nodes_in_col) in &col_groups {
        let max_w = nodes_in_col
            .iter()
            .map(|id| node_dims[id].0)
            .fold(0.0f32, f32::max);
        let total_h = nodes_in_col.iter().map(|id| node_dims[id].1).sum::<f32>()
            + (nodes_in_col.len().saturating_sub(1) as f32) * V_GAP;
        col_max_width.insert(key, max_w);
        col_total_height.insert(key, total_h);
    }

    // Compute x offset per (band, col): cumulative sum of widths + H_GAP within the band
    let mut col_x: HashMap<(usize, usize), f32> = HashMap::new();
    for b in 0..=max_band {
        let mut cursor_x = ORIGIN_X;
        for c in 0..MAX_COLS {
            let key = (b, c);
            col_x.insert(key, cursor_x);
            if col_groups.contains_key(&key) {
                cursor_x += col_max_width.get(&key).copied().unwrap_or(0.0) + H_GAP;
            }
        }
    }

    // Compute band_start_y: based on the tallest column in each band
    let mut band_start_y: Vec<f32> = vec![0.0; max_band + 2];
    band_start_y[0] = ORIGIN_Y;
    for b in 0..=max_band {
        let max_col_h = (0..MAX_COLS)
            .filter_map(|c| col_total_height.get(&(b, c)).copied())
            .fold(0.0f32, f32::max);
        band_start_y[b + 1] = band_start_y[b] + max_col_h.max(1.0) + BAND_GAP;
    }

    // Assign positions: within each column, stack nodes top-to-bottom using actual heights
    let mut positions: HashMap<String, (f32, f32)> = HashMap::new();
    for (&(band, col_in_band), nodes_in_col) in &col_groups {
        let x = col_x[&(band, col_in_band)];
        let mut cursor_y = band_start_y[band];
        for node_id in nodes_in_col {
            positions.insert(node_id.clone(), (x, cursor_y));
            cursor_y += node_dims[node_id].1 + V_GAP;
        }
    }

    // Apply positions to nodes
    for node in &mut graph.nodes {
        if let Some(&(x, y)) = positions.get(&node.id) {
            node.position = Some(GraphPosition { x, y });
        }
    }
}

pub fn build_definition_from_graph(graph: &NodeGraph) -> NodeGraphDefinition {
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for (id, node) in &graph.nodes {
        nodes.push(node_to_definition(id, node.as_ref()));
    }

    let mut output_producers: HashMap<String, String> = HashMap::new();
    for (node_id, node) in &graph.nodes {
        for port in node.output_ports() {
            output_producers.insert(port.name, node_id.clone());
        }
    }

    let mut edges = Vec::new();
    for (node_id, node) in &graph.nodes {
        for port in node.input_ports() {
            if let Some(producer) = output_producers.get(&port.name) {
                if producer != node_id {
                    edges.push(EdgeDefinition {
                        from_node_id: producer.clone(),
                        from_port: port.name.clone(),
                        to_node_id: node_id.clone(),
                        to_port: port.name.clone(),
                    });
                }
            }
        }
    }

    NodeGraphDefinition {
        nodes,
        edges,
        hyperparameter_groups: Vec::new(),
        hyperparameters: Vec::new(),
        variables: Vec::new(),
        metadata: Default::default(),
        execution_results: HashMap::new(),
    }
}

fn node_to_definition(id: &str, node: &dyn Node) -> NodeDefinition {
    NodeDefinition {
        id: id.to_string(),
        name: node.name().to_string(),
        description: node.description().map(|s| s.to_string()),
        node_type: format!("{:?}", node.node_type()),
        input_ports: node.input_ports(),
        output_ports: node.output_ports(),
        dynamic_input_ports: node.has_dynamic_input_ports(),
        dynamic_output_ports: node.has_dynamic_output_ports(),
        position: None,
        size: None,
        inline_values: HashMap::new(),
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
        disabled: false,
    }
}

impl NodeGraphDefinition {
    pub fn from_node_graph(graph: &NodeGraph) -> Self {
        build_definition_from_graph(graph)
    }

    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
