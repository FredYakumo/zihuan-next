use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::node::{DataValue, Node, NodeGraph, Port};
use crate::node::data_value::DataType;

/// A graph-level hyperparameter (variable) that can be bound to node input ports.
/// Values are NOT stored here – they live in a separate per-graph YAML file
/// in the central app-data directory (`hyperparam_store`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperParameter {
    /// Unique name within the graph
    pub name: String,
    /// Must be one of: String, Integer, Float, Boolean
    pub data_type: DataType,
    /// Whether execution is blocked when this hyperparameter has no value
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeGraphDefinition {
    pub nodes: Vec<NodeDefinition>,
    pub edges: Vec<EdgeDefinition>,
    #[serde(default)]
    pub hyperparameters: Vec<HyperParameter>,
    #[serde(skip)]
    pub execution_results: HashMap<String, HashMap<String, DataValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDefinition {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub node_type: String,
    pub input_ports: Vec<Port>,
    pub output_ports: Vec<Port>,
    pub position: Option<GraphPosition>,
    pub size: Option<GraphSize>,
    #[serde(default)]
    pub inline_values: HashMap<String, Value>,
    #[serde(default)]
    pub port_bindings: HashMap<String, String>,
    #[serde(default)]
    pub has_error: bool,
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

pub fn load_graph_definition_from_json(path: impl AsRef<Path>) -> Result<NodeGraphDefinition> {
    let content = fs::read_to_string(path.as_ref())?;
    // Backward-compat: replace removed type names before parsing so old saved graphs load cleanly.
    // refresh_port_types() will then overwrite these with the live registry types.
    let content = content
           .replace("\"data_type\": \"MessageList\"", "\"data_type\": {\"Vec\":\"OpenAIMessage\"}")
           .replace("\"data_type\":\"MessageList\"", "\"data_type\":{\"Vec\":\"OpenAIMessage\"}")
        .replace("\"data_type\": \"QQMessageList\"", "\"data_type\": {\"Vec\":\"QQMessage\"}")
        .replace("\"data_type\":\"QQMessageList\"", "\"data_type\":{\"Vec\":\"QQMessage\"}")
        // Also migrate old "List" variant name (renamed to "Vec")
        .replace("\"data_type\": {\"List\":", "\"data_type\": {\"Vec\":")
        .replace("\"data_type\":{\"List\":", "\"data_type\":{\"Vec\":");
    let mut graph: NodeGraphDefinition = serde_json::from_str(&content)?;
    refresh_port_types(&mut graph);
    Ok(graph)
}

/// Refresh port `data_type` fields in a loaded graph by looking up the canonical types from
/// the node registry. This migrates graphs saved with stale port types (e.g. `String` instead
/// of `Password`) without requiring a manual file edit.
pub fn refresh_port_types(graph: &mut NodeGraphDefinition) {
    use crate::node::registry::NODE_REGISTRY;
    for node in &mut graph.nodes {
        if let Some((canonical_inputs, canonical_outputs)) =
            NODE_REGISTRY.get_node_ports(&node.node_type)
        {
            for port in &mut node.input_ports {
                if let Some(canonical) = canonical_inputs.iter().find(|p| p.name == port.name) {
                    port.data_type = canonical.data_type.clone();
                }
            }
            for port in &mut node.output_ports {
                if let Some(canonical) = canonical_outputs.iter().find(|p| p.name == port.name) {
                    port.data_type = canonical.data_type.clone();
                }
            }
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
        Self { severity: "error".into(), message: msg.into() }
    }
    fn warning(msg: impl Into<String>) -> Self {
        Self { severity: "warning".into(), message: msg.into() }
    }
}

/// Validate a loaded `NodeGraphDefinition` against the live node registry.
/// Returns a (possibly empty) list of issues. Does NOT mutate the definition.
pub fn validate_graph_definition(graph: &NodeGraphDefinition) -> Vec<ValidationIssue> {
    use crate::node::registry::NODE_REGISTRY;
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
                // Check for ports in registry but missing from JSON (inputs)
                for canon_port in &canonical_inputs {
                    if !node.input_ports.iter().any(|p| p.name == canon_port.name) {
                        issues.push(ValidationIssue::warning(format!(
                            "节点 \"{}\" 缺少输入端口 \"{}\"",
                            node.name, canon_port.name
                        )));
                    }
                }
                // Check for ports in JSON but absent from registry (inputs)
                for port in &node.input_ports {
                    if !canonical_inputs.iter().any(|p| p.name == port.name) {
                        issues.push(ValidationIssue::warning(format!(
                            "节点 \"{}\" 存在已删除的输入端口 \"{}\"",
                            node.name, port.name
                        )));
                    }
                }
                // Check for ports in registry but missing from JSON (outputs)
                for canon_port in &canonical_outputs {
                    if !node.output_ports.iter().any(|p| p.name == canon_port.name) {
                        issues.push(ValidationIssue::warning(format!(
                            "节点 \"{}\" 缺少输出端口 \"{}\"",
                            node.name, canon_port.name
                        )));
                    }
                }
                // Check for ports in JSON but absent from registry (outputs)
                for port in &node.output_ports {
                    if !canonical_outputs.iter().any(|p| p.name == port.name) {
                        issues.push(ValidationIssue::warning(format!(
                            "节点 \"{}\" 存在已删除的输出端口 \"{}\"",
                            node.name, port.name
                        )));
                    }
                }
                // Check inline_values keys against all known port names
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
/// - Unregistered node type → mark `has_error = true` (preserved for user inspection)
/// - Missing ports vs registry → add the canonical port definition
/// - Extra ports not in registry → remove them; also drop any edges and inline_values referencing them
/// - Invalid edges (bad node/port reference) → remove
/// - Orphan inline_values (no matching port) → remove
pub fn auto_fix_graph_definition(graph: &mut NodeGraphDefinition) {
    use crate::node::registry::NODE_REGISTRY;

    // Track (node_id, port_name, is_input) of ports that no longer exist after fix –
    // used to prune dangling edges.
    let mut removed_output_ports: Vec<(String, String)> = Vec::new();
    let mut removed_input_ports: Vec<(String, String)> = Vec::new();

    for node in &mut graph.nodes {
        match NODE_REGISTRY.get_node_ports(&node.node_type) {
            None => {
                // Unknown node type — preserve it but flag the error
                node.has_error = true;
            }
            Some((canonical_inputs, canonical_outputs)) => {
                node.has_error = false;

                // Remove input ports not present in registry
                let before: Vec<String> = node.input_ports.iter().map(|p| p.name.clone()).collect();
                node.input_ports.retain(|p| canonical_inputs.iter().any(|c| c.name == p.name));
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

                // Remove output ports not present in registry
                let before: Vec<String> =
                    node.output_ports.iter().map(|p| p.name.clone()).collect();
                node.output_ports.retain(|p| canonical_outputs.iter().any(|c| c.name == p.name));
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

                // Remove orphan inline_values (no matching port in registry)
                let all_canonical_names: Vec<&str> = canonical_inputs
                    .iter()
                    .chain(canonical_outputs.iter())
                    .map(|p| p.name.as_str())
                    .collect();
                node.inline_values.retain(|k, _| all_canonical_names.contains(&k.as_str()));
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
    node.size.as_ref().map_or(MIN_WIDTH, |s| s.width.max(MIN_WIDTH))
}

/// Auto-layout all nodes in a hierarchical left-to-right arrangement following data flow.
/// Roots are placed at the leftmost column; each additional level moves one column right.
/// Multiple root chains are stacked vertically. Chains longer than MAX_COLS wrap to a new band below.
/// Node sizes are taken into account for spacing.
pub fn auto_layout(graph: &mut NodeGraphDefinition) {
    const ORIGIN_X: f32 = 40.0;
    const ORIGIN_Y: f32 = 40.0;
    const H_GAP: f32 = 60.0;   // horizontal gap between columns
    const V_GAP: f32 = 20.0;   // vertical gap between nodes in the same column
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
        discovery_order.entry(node.id.clone()).or_insert(order_counter);
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
        let total_h = nodes_in_col
            .iter()
            .map(|id| node_dims[id].1)
            .sum::<f32>()
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
        hyperparameters: Vec::new(),
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
        position: None,
        size: None,
        inline_values: HashMap::new(),
        port_bindings: HashMap::new(),
        has_error: false,
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
