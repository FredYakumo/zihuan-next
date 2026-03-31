use std::collections::{HashMap, HashSet};

use crate::node::function_graph::{
    default_embedded_function_config, embedded_function_config_from_node,
    is_function_boundary_node, sync_function_node_definition, sync_function_subgraph_signature,
    FunctionPortDef, FUNCTION_CONFIG_PORT, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use crate::node::graph_io::{
    EdgeDefinition, GraphPosition, NodeDefinition, NodeGraphDefinition, PortBindingKind,
};
use crate::node::util::set_variable::{SET_VARIABLE_NAME_PORT, SET_VARIABLE_TYPE_PORT};
use crate::node::registry::NODE_REGISTRY;
use crate::node::DataType;
use crate::ui::node_graph_view_geometry::snap_to_grid;
use crate::ui::node_graph_view_inline::{
    apply_inline_inputs_to_graph, build_inline_inputs_from_graph,
};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

#[derive(Debug, Clone)]
pub(crate) struct NodeClipboard {
    pub(crate) nodes: Vec<NodeDefinition>,
    pub(crate) edges: Vec<EdgeDefinition>,
}

#[derive(Debug, Clone)]
pub(crate) struct PasteResult {
    pub(crate) nodes: Vec<NodeDefinition>,
    pub(crate) edges: Vec<EdgeDefinition>,
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,
    pub(crate) pasted_node_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ConvertSelectionToFunctionResult {
    pub(crate) graph: NodeGraphDefinition,
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,
    pub(crate) function_node_id: String,
}

#[derive(Debug, Clone)]
struct BoundaryInputGroup {
    from_node_id: String,
    from_port: String,
    from_node_name: String,
    data_type: DataType,
    targets: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct BoundaryOutputGroup {
    from_node_id: String,
    from_port: String,
    from_node_name: String,
    data_type: DataType,
    targets: Vec<(String, String)>,
}

pub(crate) fn copy_selected_nodes_to_clipboard(
    graph: &NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
    selected_node_ids: &HashSet<String>,
) -> Option<NodeClipboard> {
    if selected_node_ids.is_empty() {
        return None;
    }

    let mut graph_clone = graph.clone();
    crate::node::graph_io::ensure_positions(&mut graph_clone);
    apply_inline_inputs_to_graph(&mut graph_clone, inline_inputs);

    let mut nodes: Vec<NodeDefinition> = graph_clone
        .nodes
        .iter()
        .filter(|node| {
            selected_node_ids.contains(&node.id) && !is_function_boundary_node(&node.node_type)
        })
        .cloned()
        .collect();

    if nodes.is_empty() {
        return None;
    }

    for node in &mut nodes {
        node.has_error = false;
        node.has_cycle = false;
    }

    let edges = graph_clone
        .edges
        .iter()
        .filter(|edge| {
            selected_node_ids.contains(&edge.from_node_id)
                && selected_node_ids.contains(&edge.to_node_id)
        })
        .cloned()
        .collect();

    Some(NodeClipboard { nodes, edges })
}

pub(crate) fn paste_nodes_from_clipboard(
    graph: &NodeGraphDefinition,
    clipboard: &NodeClipboard,
    anchor_x: f32,
    anchor_y: f32,
) -> Option<PasteResult> {
    if clipboard.nodes.is_empty() {
        return None;
    }

    let source_min_x = clipboard
        .nodes
        .iter()
        .filter_map(|node| node.position.as_ref().map(|pos| pos.x))
        .reduce(f32::min)?;
    let source_min_y = clipboard
        .nodes
        .iter()
        .filter_map(|node| node.position.as_ref().map(|pos| pos.y))
        .reduce(f32::min)?;

    let anchor_x = snap_to_grid(anchor_x);
    let anchor_y = snap_to_grid(anchor_y);
    let offset_x = anchor_x - source_min_x;
    let offset_y = anchor_y - source_min_y;

    let target_hyperparameters: HashSet<&str> = graph
        .hyperparameters
        .iter()
        .map(|hp| hp.name.as_str())
        .collect();
    let target_variables: HashSet<&str> = graph
        .variables
        .iter()
        .map(|variable| variable.name.as_str())
        .collect();

    let mut used_ids: HashSet<String> = graph.nodes.iter().map(|node| node.id.clone()).collect();
    for node in &clipboard.nodes {
        used_ids.insert(node.id.clone());
    }

    let mut id_map = HashMap::new();
    let mut pasted_nodes = Vec::with_capacity(clipboard.nodes.len());
    let mut pasted_node_ids = Vec::with_capacity(clipboard.nodes.len());

    for source_node in &clipboard.nodes {
        let new_id = next_available_node_id(&mut used_ids);
        id_map.insert(source_node.id.clone(), new_id.clone());

        let mut pasted_node = source_node.clone();
        pasted_node.id = new_id.clone();
        pasted_node.has_error = false;
        pasted_node.has_cycle = false;

        if let Some(position) = pasted_node.position.as_mut() {
            position.x = snap_to_grid(position.x + offset_x);
            position.y = snap_to_grid(position.y + offset_y);
        }

        pasted_node
            .port_bindings
            .retain(|_, binding| {
                match binding.kind {
                    PortBindingKind::Hyperparameter => {
                        target_hyperparameters.contains(binding.name.as_str())
                    }
                    PortBindingKind::Variable => {
                        target_variables.contains(binding.name.as_str())
                    }
                }
            });
        if pasted_node.node_type == "set_variable" {
            let selected = pasted_node
                .inline_values
                .get(SET_VARIABLE_NAME_PORT)
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            if let Some(selected) = selected {
                if !target_variables.contains(selected.as_str()) {
                    pasted_node.inline_values.remove(SET_VARIABLE_NAME_PORT);
                    pasted_node.inline_values.remove(SET_VARIABLE_TYPE_PORT);
                }
            }
        }

        pasted_node_ids.push(new_id);
        pasted_nodes.push(pasted_node);
    }

    let pasted_edges = clipboard
        .edges
        .iter()
        .filter_map(|edge| {
            Some(EdgeDefinition {
                from_node_id: id_map.get(&edge.from_node_id)?.clone(),
                from_port: edge.from_port.clone(),
                to_node_id: id_map.get(&edge.to_node_id)?.clone(),
                to_port: edge.to_port.clone(),
            })
        })
        .collect::<Vec<_>>();

    let temp_graph = NodeGraphDefinition {
        nodes: pasted_nodes.clone(),
        edges: pasted_edges.clone(),
        hyperparameter_groups: Vec::new(),
        hyperparameters: Vec::new(),
        variables: Vec::new(),
        execution_results: HashMap::new(),
    };
    let inline_inputs = build_inline_inputs_from_graph(&temp_graph);

    Some(PasteResult {
        nodes: pasted_nodes,
        edges: pasted_edges,
        inline_inputs,
        pasted_node_ids,
    })
}

pub(crate) fn convert_selection_to_function_subgraph(
    graph: &NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
    selected_node_ids: &HashSet<String>,
) -> Result<ConvertSelectionToFunctionResult, String> {
    if selected_node_ids.is_empty() {
        return Err("请先选择要转换的节点".to_string());
    }

    let mut graph_clone = graph.clone();
    crate::node::graph_io::ensure_positions(&mut graph_clone);
    apply_inline_inputs_to_graph(&mut graph_clone, inline_inputs);
    // 确保所有节点的端口类型与注册表一致，避免因旧 JSON 保存了错误类型
    // （如 BotAdapterRef 被存成 String）导致边界类型推断出错。
    crate::node::graph_io::refresh_port_types(&mut graph_clone);

    let selected_nodes = graph_clone
        .nodes
        .iter()
        .filter(|node| selected_node_ids.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    if selected_nodes.is_empty() {
        return Err("未找到选中的节点".to_string());
    }

    for node in &selected_nodes {
        if is_function_boundary_node(&node.node_type) {
            return Err("函数边界节点不能转换为函数子图".to_string());
        }
        if NODE_REGISTRY.is_event_producer(&node.node_type) {
            return Err(format!(
                "选中的节点包含事件源，无法转换为函数子图：{} ({})",
                node.name, node.node_type
            ));
        }
    }

    let selected_node_map = selected_nodes
        .iter()
        .map(|node| (node.id.clone(), node.clone()))
        .collect::<HashMap<_, _>>();

    let selection_min_x = selected_nodes
        .iter()
        .filter_map(|node| node.position.as_ref().map(|position| position.x))
        .reduce(f32::min)
        .ok_or_else(|| "选中的节点缺少位置信息".to_string())?;
    let selection_min_y = selected_nodes
        .iter()
        .filter_map(|node| node.position.as_ref().map(|position| position.y))
        .reduce(f32::min)
        .ok_or_else(|| "选中的节点缺少位置信息".to_string())?;

    let selection_max_x = selected_nodes
        .iter()
        .filter_map(|node| node.position.as_ref().map(|position| position.x))
        .reduce(f32::max)
        .ok_or_else(|| "选中的节点缺少位置信息".to_string())?;

    let mut internal_edges = Vec::new();
    let mut input_groups = Vec::<BoundaryInputGroup>::new();
    let mut input_group_index = HashMap::<(String, String), usize>::new();
    let mut output_groups = Vec::<BoundaryOutputGroup>::new();
    let mut output_group_index = HashMap::<(String, String), usize>::new();

    for edge in &graph_clone.edges {
        let from_selected = selected_node_ids.contains(&edge.from_node_id);
        let to_selected = selected_node_ids.contains(&edge.to_node_id);
        match (from_selected, to_selected) {
            (true, true) => internal_edges.push(edge.clone()),
            (false, true) => {
                let Some(source_node) = graph_clone
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.from_node_id)
                else {
                    return Err(format!("未找到边的源节点：{}", edge.from_node_id));
                };
                let Some(data_type) = find_output_port_type(source_node, &edge.from_port) else {
                    return Err(format!(
                        "未找到输入边源端口类型：{}:{}",
                        source_node.name, edge.from_port
                    ));
                };

                let key = (edge.from_node_id.clone(), edge.from_port.clone());
                let group_index = if let Some(index) = input_group_index.get(&key) {
                    *index
                } else {
                    let index = input_groups.len();
                    input_groups.push(BoundaryInputGroup {
                        from_node_id: edge.from_node_id.clone(),
                        from_port: edge.from_port.clone(),
                        from_node_name: source_node.name.clone(),
                        data_type,
                        targets: Vec::new(),
                    });
                    input_group_index.insert(key, index);
                    index
                };
                input_groups[group_index]
                    .targets
                    .push((edge.to_node_id.clone(), edge.to_port.clone()));
            }
            (true, false) => {
                let Some(source_node) = selected_node_map.get(&edge.from_node_id) else {
                    return Err(format!("未找到边的选中源节点：{}", edge.from_node_id));
                };
                let Some(data_type) = find_output_port_type(source_node, &edge.from_port) else {
                    return Err(format!(
                        "未找到输出边源端口类型：{}:{}",
                        source_node.name, edge.from_port
                    ));
                };

                let key = (edge.from_node_id.clone(), edge.from_port.clone());
                let group_index = if let Some(index) = output_group_index.get(&key) {
                    *index
                } else {
                    let index = output_groups.len();
                    output_groups.push(BoundaryOutputGroup {
                        from_node_id: edge.from_node_id.clone(),
                        from_port: edge.from_port.clone(),
                        from_node_name: source_node.name.clone(),
                        data_type,
                        targets: Vec::new(),
                    });
                    output_group_index.insert(key, index);
                    index
                };
                output_groups[group_index]
                    .targets
                    .push((edge.to_node_id.clone(), edge.to_port.clone()));
            }
            (false, false) => {}
        }
    }

    let mut used_input_names = HashSet::new();
    let input_ports = input_groups
        .iter()
        .map(|group| {
            let same_target_name = group
                .targets
                .first()
                .map(|(_, port_name)| {
                    group
                        .targets
                        .iter()
                        .all(|(_, current_name)| current_name == port_name)
                })
                .unwrap_or(false);

            let preferred = if same_target_name {
                group.targets
                    .first()
                    .map(|(_, port_name)| port_name.as_str())
                    .unwrap_or("input")
            } else {
                ""
            };
            let fallback = format!("{}_{}", group.from_node_name, group.from_port);
            FunctionPortDef {
                name: unique_port_name(
                    &mut used_input_names,
                    &[preferred, fallback.as_str(), group.from_port.as_str(), "input"],
                ),
                data_type: group.data_type.clone(),
            }
        })
        .collect::<Vec<_>>();

    let mut used_output_names = HashSet::new();
    let output_ports = output_groups
        .iter()
        .map(|group| {
            let fallback = format!("{}_{}", group.from_node_name, group.from_port);
            FunctionPortDef {
                name: unique_port_name(
                    &mut used_output_names,
                    &[group.from_port.as_str(), fallback.as_str(), "output"],
                ),
                data_type: group.data_type.clone(),
            }
        })
        .collect::<Vec<_>>();

    let mut subgraph_nodes = selected_nodes.clone();
    let offset_x = 220.0 - selection_min_x;
    let offset_y = 80.0 - selection_min_y;
    for node in &mut subgraph_nodes {
        node.has_error = false;
        node.has_cycle = false;
        if let Some(position) = node.position.as_mut() {
            position.x = snap_to_grid(position.x + offset_x);
            position.y = snap_to_grid(position.y + offset_y);
        }
    }

    let mut subgraph = NodeGraphDefinition {
        nodes: subgraph_nodes,
        edges: internal_edges,
        hyperparameter_groups: Vec::new(),
        hyperparameters: Vec::new(),
        variables: Vec::new(),
        execution_results: HashMap::new(),
    };
    sync_function_subgraph_signature(&mut subgraph, &input_ports, &output_ports);

    for (group, port) in input_groups.iter().zip(input_ports.iter()) {
        for (to_node_id, to_port) in &group.targets {
            subgraph.edges.push(EdgeDefinition {
                from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
                from_port: port.name.clone(),
                to_node_id: to_node_id.clone(),
                to_port: to_port.clone(),
            });
        }
    }

    for (group, port) in output_groups.iter().zip(output_ports.iter()) {
        subgraph.edges.push(EdgeDefinition {
            from_node_id: group.from_node_id.clone(),
            from_port: group.from_port.clone(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: port.name.clone(),
        });
    }

    let content_max_x = subgraph
        .nodes
        .iter()
        .filter(|node| node.id != FUNCTION_INPUTS_NODE_ID && node.id != FUNCTION_OUTPUTS_NODE_ID)
        .filter_map(|node| node.position.as_ref().map(|position| position.x))
        .reduce(f32::max)
        .unwrap_or(220.0);
    if let Some(node) = subgraph
        .nodes
        .iter_mut()
        .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
    {
        node.position = Some(GraphPosition { x: 60.0, y: 80.0 });
    }
    if let Some(node) = subgraph
        .nodes
        .iter_mut()
        .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
    {
        node.position = Some(GraphPosition {
            x: snap_to_grid(content_max_x + 260.0),
            y: 80.0,
        });
    }

    let function_name = next_available_display_name(graph, "提取函数");
    let function_node_id = {
        let mut used_ids = graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        next_available_node_id(&mut used_ids)
    };

    let mut config = default_embedded_function_config(function_name.clone());
    config.inputs = input_ports.clone();
    config.outputs = output_ports.clone();
    config.subgraph = subgraph;

    let dummy_node = NODE_REGISTRY
        .create_node("function", &function_node_id, &function_name)
        .map_err(|error| format!("创建函数节点失败: {error}"))?;
    let mut function_node = NodeDefinition {
        id: function_node_id.clone(),
        name: function_name.clone(),
        description: dummy_node.description().map(|text| text.to_string()),
        node_type: "function".to_string(),
        input_ports: dummy_node.input_ports(),
        output_ports: dummy_node.output_ports(),
        dynamic_input_ports: dummy_node.has_dynamic_input_ports(),
        dynamic_output_ports: dummy_node.has_dynamic_output_ports(),
        position: Some(GraphPosition {
            x: snap_to_grid(selection_min_x),
            y: snap_to_grid(selection_min_y),
        }),
        size: None,
        inline_values: HashMap::new(),
        port_bindings: HashMap::new(),
        has_error: false,
        has_cycle: false,
    };
    sync_function_node_definition(&mut function_node, &config);
    function_node.position = Some(GraphPosition {
        x: snap_to_grid(selection_min_x),
        y: snap_to_grid(selection_min_y),
    });

    let mut final_graph = graph_clone;
    final_graph
        .nodes
        .retain(|node| !selected_node_ids.contains(&node.id));
    final_graph.edges.retain(|edge| {
        !selected_node_ids.contains(&edge.from_node_id) && !selected_node_ids.contains(&edge.to_node_id)
    });
    final_graph.nodes.push(function_node.clone());

    for (group, port) in input_groups.iter().zip(input_ports.iter()) {
        final_graph.edges.push(EdgeDefinition {
            from_node_id: group.from_node_id.clone(),
            from_port: group.from_port.clone(),
            to_node_id: function_node_id.clone(),
            to_port: port.name.clone(),
        });
    }

    for (group, port) in output_groups.iter().zip(output_ports.iter()) {
        for (to_node_id, to_port) in &group.targets {
            final_graph.edges.push(EdgeDefinition {
                from_node_id: function_node_id.clone(),
                from_port: port.name.clone(),
                to_node_id: to_node_id.clone(),
                to_port: to_port.clone(),
            });
        }
    }

    let existing_node_ids = final_graph
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    final_graph
        .execution_results
        .retain(|node_id, _| existing_node_ids.contains(node_id.as_str()));

    let mut rebuilt_inline_inputs = build_inline_inputs_from_graph(&final_graph);
    if let Some(config_value) = function_node.inline_values.get(FUNCTION_CONFIG_PORT).cloned() {
        rebuilt_inline_inputs.insert(
            inline_port_key(&function_node_id, FUNCTION_CONFIG_PORT),
            InlinePortValue::Json(config_value),
        );
    } else if let Some(config) = embedded_function_config_from_node(&function_node) {
        if let Ok(value) = serde_json::to_value(config) {
            rebuilt_inline_inputs.insert(
                inline_port_key(&function_node_id, FUNCTION_CONFIG_PORT),
                InlinePortValue::Json(value),
            );
        }
    }

    let _ = selection_max_x;

    Ok(ConvertSelectionToFunctionResult {
        graph: final_graph,
        inline_inputs: rebuilt_inline_inputs,
        function_node_id,
    })
}

fn next_available_node_id(used_ids: &mut HashSet<String>) -> String {
    let mut index = 1usize;
    loop {
        let candidate = format!("node_{index}");
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn next_available_display_name(graph: &NodeGraphDefinition, base: &str) -> String {
    let existing_names = graph
        .nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<HashSet<_>>();
    if !existing_names.contains(base) {
        return base.to_string();
    }

    let mut index = 2usize;
    loop {
        let candidate = format!("{base} {index}");
        if !existing_names.contains(candidate.as_str()) {
            return candidate;
        }
        index += 1;
    }
}

fn find_output_port_type(node: &NodeDefinition, port_name: &str) -> Option<DataType> {
    node.output_ports
        .iter()
        .find(|port| port.name == port_name)
        .map(|port| port.data_type.clone())
}

fn unique_port_name(used_names: &mut HashSet<String>, candidates: &[&str]) -> String {
    for candidate in candidates {
        let normalized = normalize_port_name(candidate);
        if normalized.is_empty() {
            continue;
        }
        if used_names.insert(normalized.clone()) {
            return normalized;
        }
    }

    let base = "port".to_string();
    if used_names.insert(base.clone()) {
        return base;
    }

    let mut index = 2usize;
    loop {
        let candidate = format!("port_{index}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

fn normalize_port_name(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = true;
    let mut previous_was_lower_or_digit = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() && previous_was_lower_or_digit && !normalized.ends_with('_') {
                normalized.push('_');
            }
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            previous_was_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !previous_was_separator && !normalized.is_empty() {
            normalized.push('_');
            previous_was_separator = true;
            previous_was_lower_or_digit = false;
        }
    }

    while normalized.ends_with('_') {
        normalized.pop();
    }

    if normalized.is_empty() {
        return String::new();
    }

    if normalized
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("port_{normalized}")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::node::function_graph::{
        embedded_function_config_from_node, FunctionPortDef, FUNCTION_INPUTS_NODE_ID,
        FUNCTION_INPUTS_NODE_TYPE, FUNCTION_OUTPUTS_NODE_ID,
    };
    use crate::node::data_value::DataType;
    use crate::node::graph_io::{
        EdgeDefinition, GraphPosition, HyperParameter, NodeDefinition, NodeGraphDefinition,
    };
    use crate::node::Port;
    use crate::ui::node_render::InlinePortValue;

    use super::{
        convert_selection_to_function_subgraph, copy_selected_nodes_to_clipboard,
        normalize_port_name, paste_nodes_from_clipboard,
    };

    fn ensure_registry_initialized() {
        let _ = crate::node::registry::init_node_registry();
    }

    fn node_at(id: &str, x: f32, y: f32) -> NodeDefinition {
        NodeDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            node_type: "preview_string".to_string(),
            input_ports: vec![Port::new("text", DataType::String)],
            output_ports: vec![Port::new("value", DataType::String)],
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: Some(GraphPosition { x, y }),
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        }
    }

    fn custom_node(
        id: &str,
        name: &str,
        x: f32,
        y: f32,
        input_ports: Vec<Port>,
        output_ports: Vec<Port>,
    ) -> NodeDefinition {
        NodeDefinition {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            node_type: "preview_string".to_string(),
            input_ports,
            output_ports,
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: Some(GraphPosition { x, y }),
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
            has_cycle: false,
        }
    }

    #[test]
    fn copy_keeps_only_internal_edges() {
        let graph = NodeGraphDefinition {
            nodes: vec![
                node_at("node_1", 40.0, 40.0),
                node_at("node_2", 140.0, 40.0),
                node_at("node_3", 240.0, 40.0),
            ],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_2".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_2".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_3".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
        hyperparameters: Vec::new(),
        variables: Vec::new(),
        execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_1".to_string(), "node_2".to_string()]);
        let clipboard =
            copy_selected_nodes_to_clipboard(&graph, &HashMap::new(), &selected).unwrap();

        assert_eq!(clipboard.nodes.len(), 2);
        assert_eq!(clipboard.edges.len(), 1);
        assert_eq!(clipboard.edges[0].from_node_id, "node_1");
        assert_eq!(clipboard.edges[0].to_node_id, "node_2");
    }

    #[test]
    fn paste_remaps_ids_and_preserves_relative_layout() {
        let clipboard_graph = NodeGraphDefinition {
            nodes: vec![
                node_at("node_1", 40.0, 60.0),
                node_at("node_2", 140.0, 160.0),
            ],
            edges: vec![EdgeDefinition {
                from_node_id: "node_1".to_string(),
                from_port: "value".to_string(),
                to_node_id: "node_2".to_string(),
                to_port: "text".to_string(),
            }],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };
        let selected = HashSet::from(["node_1".to_string(), "node_2".to_string()]);
        let clipboard =
            copy_selected_nodes_to_clipboard(&clipboard_graph, &HashMap::new(), &selected).unwrap();

        let target_graph = NodeGraphDefinition {
            nodes: vec![node_at("node_9", 0.0, 0.0)],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let pasted = paste_nodes_from_clipboard(&target_graph, &clipboard, 200.0, 300.0).unwrap();

        assert_eq!(pasted.nodes.len(), 2);
        assert_eq!(pasted.edges.len(), 1);
        assert_ne!(pasted.nodes[0].id, "node_1");
        assert_ne!(pasted.nodes[1].id, "node_2");
        assert_eq!(pasted.nodes[0].position.as_ref().unwrap().x, 200.0);
        assert_eq!(pasted.nodes[0].position.as_ref().unwrap().y, 300.0);
        assert_eq!(pasted.nodes[1].position.as_ref().unwrap().x, 300.0);
        assert_eq!(pasted.nodes[1].position.as_ref().unwrap().y, 400.0);
        assert_eq!(pasted.edges[0].from_node_id, pasted.nodes[0].id);
        assert_eq!(pasted.edges[0].to_node_id, pasted.nodes[1].id);
    }

    #[test]
    fn paste_rebuilds_inline_inputs_for_new_ids() {
        let mut inline_inputs = HashMap::new();
        inline_inputs.insert(
            "node_1::text".to_string(),
            InlinePortValue::Text("hello".to_string()),
        );

        let clipboard_graph = NodeGraphDefinition {
            nodes: vec![node_at("node_1", 40.0, 40.0)],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };
        let selected = HashSet::from(["node_1".to_string()]);
        let clipboard =
            copy_selected_nodes_to_clipboard(&clipboard_graph, &inline_inputs, &selected).unwrap();

        let pasted =
            paste_nodes_from_clipboard(&NodeGraphDefinition::default(), &clipboard, 200.0, 200.0)
                .unwrap();

        let new_key = format!("{}::text", pasted.pasted_node_ids[0]);
        match pasted.inline_inputs.get(&new_key) {
            Some(InlinePortValue::Text(value)) => assert_eq!(value, "hello"),
            other => panic!("unexpected inline input: {other:?}"),
        }
    }

    #[test]
    fn paste_clears_missing_hyperparameter_bindings() {
        let mut node = node_at("node_1", 40.0, 40.0);
        node.port_bindings
            .insert(
                "text".to_string(),
                crate::node::graph_io::PortBinding::hyperparameter("missing_hp".to_string()),
            );

        let clipboard_graph = NodeGraphDefinition {
            nodes: vec![node],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };
        let selected = HashSet::from(["node_1".to_string()]);
        let clipboard =
            copy_selected_nodes_to_clipboard(&clipboard_graph, &HashMap::new(), &selected).unwrap();

        let target_graph = NodeGraphDefinition {
            nodes: Vec::new(),
            edges: Vec::new(),
            hyperparameter_groups: vec!["default".to_string()],
            hyperparameters: vec![HyperParameter {
                name: "other_hp".to_string(),
                data_type: DataType::String,
                group: "default".to_string(),
                required: false,
                description: None,
            }],
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let pasted = paste_nodes_from_clipboard(&target_graph, &clipboard, 100.0, 100.0).unwrap();

        assert!(pasted.nodes[0].port_bindings.is_empty());
    }

    #[test]
    fn copy_ignores_function_boundary_nodes() {
        let mut boundary = node_at("node_1", 40.0, 40.0);
        boundary.node_type = FUNCTION_INPUTS_NODE_TYPE.to_string();

        let graph = NodeGraphDefinition {
            nodes: vec![boundary],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };
        let selected = HashSet::from(["node_1".to_string()]);

        let clipboard = copy_selected_nodes_to_clipboard(&graph, &HashMap::new(), &selected);

        assert!(clipboard.is_none());
    }

    #[test]
    fn convert_selection_extracts_basic_function_subgraph() {
        ensure_registry_initialized();

        let mut inner_1 = custom_node(
            "node_2",
            "Inner One",
            240.0,
            120.0,
            vec![Port::new("text", DataType::String)],
            vec![Port::new("value", DataType::String)],
        );
        inner_1
            .inline_values
            .insert("text".to_string(), serde_json::Value::String("hello".to_string()));

        let graph = NodeGraphDefinition {
            nodes: vec![
                custom_node(
                    "node_1",
                    "External Source",
                    40.0,
                    120.0,
                    Vec::new(),
                    vec![Port::new("value", DataType::String)],
                ),
                inner_1,
                custom_node(
                    "node_3",
                    "Inner Two",
                    420.0,
                    120.0,
                    vec![Port::new("text", DataType::String)],
                    vec![Port::new("value", DataType::String)],
                ),
                custom_node(
                    "node_4",
                    "External Sink",
                    700.0,
                    120.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
            ],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_2".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_2".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_3".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_3".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_4".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::from([("node_2".to_string(), HashMap::new())]),
        };

        let selected = HashSet::from(["node_2".to_string(), "node_3".to_string()]);
        let result =
            convert_selection_to_function_subgraph(&graph, &HashMap::new(), &selected).unwrap();

        assert_eq!(result.function_node_id, "node_5");
        assert_eq!(result.graph.nodes.len(), 3);
        assert_eq!(result.graph.edges.len(), 2);
        assert!(
            result
                .graph
                .nodes
                .iter()
                .all(|node| node.id != "node_2" && node.id != "node_3")
        );
        assert!(!result.graph.execution_results.contains_key("node_2"));

        let function_node = result
            .graph
            .nodes
            .iter()
            .find(|node| node.id == result.function_node_id)
            .unwrap();
        let config = embedded_function_config_from_node(function_node).unwrap();
        assert_eq!(
            config.inputs,
            vec![FunctionPortDef {
                name: "text".to_string(),
                data_type: DataType::String,
            }]
        );
        assert_eq!(
            config.outputs,
            vec![FunctionPortDef {
                name: "value".to_string(),
                data_type: DataType::String,
            }]
        );
        assert_eq!(config.subgraph.nodes.len(), 4);
        assert_eq!(config.subgraph.edges.len(), 3);
        assert!(config.subgraph.edges.iter().any(|edge| {
            edge.from_node_id == FUNCTION_INPUTS_NODE_ID
                && edge.from_port == "text"
                && edge.to_node_id == "node_2"
                && edge.to_port == "text"
        }));
        assert!(config.subgraph.edges.iter().any(|edge| {
            edge.from_node_id == "node_2"
                && edge.to_node_id == "node_3"
                && edge.from_port == "value"
                && edge.to_port == "text"
        }));
        assert!(config.subgraph.edges.iter().any(|edge| {
            edge.from_node_id == "node_3"
                && edge.from_port == "value"
                && edge.to_node_id == FUNCTION_OUTPUTS_NODE_ID
                && edge.to_port == "value"
        }));
        assert!(result.graph.edges.iter().any(|edge| {
            edge.from_node_id == "node_1"
                && edge.from_port == "value"
                && edge.to_node_id == result.function_node_id
                && edge.to_port == "text"
        }));
        assert!(result.graph.edges.iter().any(|edge| {
            edge.from_node_id == result.function_node_id
                && edge.from_port == "value"
                && edge.to_node_id == "node_4"
                && edge.to_port == "text"
        }));
        assert!(matches!(
            result
                .inline_inputs
                .get(&format!("{}::function_config", result.function_node_id)),
            Some(InlinePortValue::Json(_))
        ));
    }

    #[test]
    fn convert_selection_groups_fanout_inputs_into_one_function_input() {
        ensure_registry_initialized();

        let graph = NodeGraphDefinition {
            nodes: vec![
                custom_node(
                    "node_1",
                    "External Source",
                    40.0,
                    120.0,
                    Vec::new(),
                    vec![Port::new("value", DataType::String)],
                ),
                custom_node(
                    "node_2",
                    "Inner One",
                    240.0,
                    40.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
                custom_node(
                    "node_3",
                    "Inner Two",
                    240.0,
                    200.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
            ],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_2".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_3".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_2".to_string(), "node_3".to_string()]);
        let result =
            convert_selection_to_function_subgraph(&graph, &HashMap::new(), &selected).unwrap();
        let function_node = result
            .graph
            .nodes
            .iter()
            .find(|node| node.id == result.function_node_id)
            .unwrap();
        let config = embedded_function_config_from_node(function_node).unwrap();

        assert_eq!(config.inputs.len(), 1);
        assert_eq!(config.inputs[0].name, "text");
        assert_eq!(result.graph.edges.len(), 1);
        assert_eq!(result.graph.edges[0].to_node_id, result.function_node_id);
        assert_eq!(
            config
                .subgraph
                .edges
                .iter()
                .filter(|edge| {
                    edge.from_node_id == FUNCTION_INPUTS_NODE_ID && edge.from_port == "text"
                })
                .count(),
            2
        );
    }

    #[test]
    fn convert_selection_groups_fanout_outputs_into_one_function_output() {
        ensure_registry_initialized();

        let graph = NodeGraphDefinition {
            nodes: vec![
                custom_node(
                    "node_1",
                    "Inner Source",
                    240.0,
                    120.0,
                    Vec::new(),
                    vec![Port::new("value", DataType::String)],
                ),
                custom_node(
                    "node_2",
                    "Sink One",
                    520.0,
                    40.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
                custom_node(
                    "node_3",
                    "Sink Two",
                    520.0,
                    200.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
            ],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_2".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "node_3".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_1".to_string()]);
        let result =
            convert_selection_to_function_subgraph(&graph, &HashMap::new(), &selected).unwrap();
        let function_node = result
            .graph
            .nodes
            .iter()
            .find(|node| node.id == result.function_node_id)
            .unwrap();
        let config = embedded_function_config_from_node(function_node).unwrap();

        assert_eq!(config.outputs.len(), 1);
        assert_eq!(config.outputs[0].name, "value");
        assert_eq!(
            result
                .graph
                .edges
                .iter()
                .filter(|edge| edge.from_node_id == result.function_node_id)
                .count(),
            2
        );
        assert!(config.subgraph.edges.iter().any(|edge| {
            edge.from_node_id == "node_1"
                && edge.from_port == "value"
                && edge.to_node_id == FUNCTION_OUTPUTS_NODE_ID
                && edge.to_port == "value"
        }));
    }

    #[test]
    fn convert_selection_supports_zero_boundary_ports() {
        ensure_registry_initialized();

        let graph = NodeGraphDefinition {
            nodes: vec![custom_node(
                "node_1",
                "Solo Node",
                240.0,
                120.0,
                vec![Port::new("text", DataType::String)],
                vec![Port::new("value", DataType::String)],
            )],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_1".to_string()]);
        let result =
            convert_selection_to_function_subgraph(&graph, &HashMap::new(), &selected).unwrap();
        let function_node = result
            .graph
            .nodes
            .iter()
            .find(|node| node.id == result.function_node_id)
            .unwrap();
        let config = embedded_function_config_from_node(function_node).unwrap();

        assert!(config.inputs.is_empty());
        assert!(config.outputs.is_empty());
        assert_eq!(result.graph.edges.len(), 0);
        assert_eq!(config.subgraph.nodes.len(), 3);
    }

    #[test]
    fn convert_selection_dedupes_and_normalizes_port_names() {
        ensure_registry_initialized();

        let graph = NodeGraphDefinition {
            nodes: vec![
                custom_node(
                    "node_1",
                    "Alpha Source",
                    40.0,
                    40.0,
                    Vec::new(),
                    vec![Port::new("Output Value", DataType::String)],
                ),
                custom_node(
                    "node_2",
                    "Beta Source",
                    40.0,
                    200.0,
                    Vec::new(),
                    vec![Port::new("Output Value", DataType::String)],
                ),
                custom_node(
                    "node_3",
                    "First Node",
                    240.0,
                    40.0,
                    vec![Port::new("text", DataType::String)],
                    vec![Port::new("Result Value", DataType::String)],
                ),
                custom_node(
                    "node_4",
                    "Second Node",
                    240.0,
                    200.0,
                    vec![Port::new("text", DataType::String)],
                    vec![Port::new("Result Value", DataType::String)],
                ),
                custom_node(
                    "node_5",
                    "Sink One",
                    520.0,
                    40.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
                custom_node(
                    "node_6",
                    "Sink Two",
                    520.0,
                    200.0,
                    vec![Port::new("text", DataType::String)],
                    Vec::new(),
                ),
            ],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "node_1".to_string(),
                    from_port: "Output Value".to_string(),
                    to_node_id: "node_3".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_2".to_string(),
                    from_port: "Output Value".to_string(),
                    to_node_id: "node_4".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_3".to_string(),
                    from_port: "Result Value".to_string(),
                    to_node_id: "node_5".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "node_4".to_string(),
                    from_port: "Result Value".to_string(),
                    to_node_id: "node_6".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_3".to_string(), "node_4".to_string()]);
        let result =
            convert_selection_to_function_subgraph(&graph, &HashMap::new(), &selected).unwrap();
        let function_node = result
            .graph
            .nodes
            .iter()
            .find(|node| node.id == result.function_node_id)
            .unwrap();
        let config = embedded_function_config_from_node(function_node).unwrap();

        assert_eq!(
            config.inputs.iter().map(|port| port.name.as_str()).collect::<Vec<_>>(),
            vec!["text", "beta_source_output_value"]
        );
        assert_eq!(
            config.outputs
                .iter()
                .map(|port| port.name.as_str())
                .collect::<Vec<_>>(),
            vec!["result_value", "second_node_result_value"]
        );
    }

    #[test]
    fn convert_selection_rejects_event_producers_and_function_boundaries() {
        ensure_registry_initialized();

        let graph_with_event = NodeGraphDefinition {
            nodes: vec![NodeDefinition {
                id: "node_1".to_string(),
                name: "Bot".to_string(),
                description: None,
                node_type: "bot_adapter".to_string(),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                dynamic_input_ports: false,
                dynamic_output_ports: false,
                position: Some(GraphPosition { x: 40.0, y: 40.0 }),
                size: None,
                inline_values: HashMap::new(),
                port_bindings: HashMap::new(),
                has_error: false,
                has_cycle: false,
            }],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };

        let selected = HashSet::from(["node_1".to_string()]);
        let event_error =
            convert_selection_to_function_subgraph(&graph_with_event, &HashMap::new(), &selected)
                .unwrap_err();
        assert!(event_error.contains("事件源"));

        let boundary_graph = NodeGraphDefinition {
            nodes: vec![NodeDefinition {
                id: "node_2".to_string(),
                name: "函数输入".to_string(),
                description: None,
                node_type: FUNCTION_INPUTS_NODE_TYPE.to_string(),
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                dynamic_input_ports: false,
                dynamic_output_ports: false,
                position: Some(GraphPosition { x: 40.0, y: 40.0 }),
                size: None,
                inline_values: HashMap::new(),
                port_bindings: HashMap::new(),
                has_error: false,
                has_cycle: false,
            }],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            variables: Vec::new(),
            execution_results: HashMap::new(),
        };
        let boundary_selected = HashSet::from(["node_2".to_string()]);
        let boundary_error =
            convert_selection_to_function_subgraph(&boundary_graph, &HashMap::new(), &boundary_selected)
                .unwrap_err();
        assert!(boundary_error.contains("边界节点"));
    }

    #[test]
    fn normalize_port_name_converts_to_snake_case() {
        assert_eq!(normalize_port_name("Output Value"), "output_value");
        assert_eq!(normalize_port_name("HTTPRequest"), "httprequest");
        assert_eq!(normalize_port_name("123 Value"), "port_123_value");
        assert_eq!(normalize_port_name("中文"), "");
    }
}



