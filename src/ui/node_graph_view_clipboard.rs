use std::collections::{HashMap, HashSet};

use crate::node::graph_io::{EdgeDefinition, NodeDefinition, NodeGraphDefinition};
use crate::ui::node_graph_view_geometry::snap_to_grid;
use crate::ui::node_graph_view_inline::{
    apply_inline_inputs_to_graph, build_inline_inputs_from_graph,
};
use crate::ui::node_render::InlinePortValue;

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
        .filter(|node| selected_node_ids.contains(&node.id))
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
            .retain(|_, hp_name| target_hyperparameters.contains(hp_name.as_str()));

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

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use crate::node::data_value::DataType;
    use crate::node::graph_io::{
        EdgeDefinition, GraphPosition, HyperParameter, NodeDefinition, NodeGraphDefinition,
    };
    use crate::node::Port;
    use crate::ui::node_render::InlinePortValue;

    use super::{copy_selected_nodes_to_clipboard, paste_nodes_from_clipboard};

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
            .insert("text".to_string(), "missing_hp".to_string());

        let clipboard_graph = NodeGraphDefinition {
            nodes: vec![node],
            edges: Vec::new(),
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
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
            execution_results: HashMap::new(),
        };

        let pasted = paste_nodes_from_clipboard(&target_graph, &clipboard, 100.0, 100.0).unwrap();

        assert!(pasted.nodes[0].port_bindings.is_empty());
    }
}
