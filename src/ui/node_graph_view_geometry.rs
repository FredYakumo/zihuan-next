use std::collections::HashSet;

use crate::node::graph_io::{NodeDefinition, NodeGraphDefinition};
use crate::node::DataType;
use crate::ui::graph_window::{EdgeCornerVm, EdgeLabelVm, EdgeSegmentVm, EdgeVm, GridLineVm};
use crate::ui::selection::SelectionState;

pub(crate) const GRID_SIZE: f32 = 20.0;
pub(crate) const NODE_WIDTH_CELLS: f32 = 10.0;
pub(crate) const NODE_HEADER_ROWS: f32 = 2.0;
pub(crate) const NODE_MIN_ROWS: f32 = 3.0;
pub(crate) const NODE_PADDING_BOTTOM: f32 = 0.8;
pub(crate) const LIST_NODE_MIN_HEIGHT: f32 = GRID_SIZE * 8.0;
pub(crate) const LIST_NODE_OUTPUT_PORT_CENTER_Y: f32 = GRID_SIZE * 1.6;
pub(crate) const BRAIN_NODE_MIN_HEIGHT: f32 = GRID_SIZE * 6.2;
pub(crate) const CANVAS_WIDTH: f32 = 4000.0;
pub(crate) const CANVAS_HEIGHT: f32 = 4000.0;
pub(crate) const EDGE_THICKNESS_RATIO: f32 = 0.3;

fn is_list_data_node(node: &NodeDefinition) -> bool {
    matches!(node.node_type.as_str(), "message_list_data" | "qq_message_list_data")
}

fn is_brain_node(node: &NodeDefinition) -> bool {
    node.node_type == "brain"
}

pub(crate) fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SIZE).round() * GRID_SIZE
}

pub(crate) fn snap_to_grid_center(value: f32) -> f32 {
    snap_to_grid(value - GRID_SIZE / 2.0) + GRID_SIZE / 2.0
}

pub(crate) fn node_dimensions(node: &NodeDefinition) -> (f32, f32) {
    let min_width = GRID_SIZE * NODE_WIDTH_CELLS;
    let port_rows = node.input_ports.len().max(node.output_ports.len()) as f32;
    let default_min_height =
        GRID_SIZE * (NODE_MIN_ROWS.max(NODE_HEADER_ROWS + port_rows) + NODE_PADDING_BOTTOM);
    let min_height = if is_list_data_node(node) {
        default_min_height.max(LIST_NODE_MIN_HEIGHT)
    } else if is_brain_node(node) {
        default_min_height.max(BRAIN_NODE_MIN_HEIGHT)
    } else {
        default_min_height
    };

    match &node.size {
        Some(size) => (size.width.max(min_width), size.height.max(min_height)),
        None => (min_width, min_height),
    }
}

pub(crate) fn get_port_center(
    graph: &NodeGraphDefinition,
    node_id: &str,
    port_name: &str,
    is_input: bool,
) -> Option<(f32, f32)> {
    let node = graph.nodes.iter().find(|n| n.id == node_id)?;
    get_port_center_for_node(node, port_name, is_input)
}

pub(crate) fn get_port_center_for_node(
    node: &NodeDefinition,
    port_name: &str,
    is_input: bool,
) -> Option<(f32, f32)> {
    let position = node.position.as_ref()?;

    let ports = if is_input { &node.input_ports } else { &node.output_ports };
    let index = ports.iter().position(|p| p.name == port_name)? as f32;
    let radius = GRID_SIZE / 2.0;
    let base_y_offset = GRID_SIZE * NODE_HEADER_ROWS;
    let (node_width, _) = node_dimensions(node);

    let center_x = if is_input {
        position.x + GRID_SIZE * 0.5
    } else {
        position.x + node_width - (GRID_SIZE * 0.5)
    };
    let center_y = if !is_input && is_list_data_node(node) {
        position.y + LIST_NODE_OUTPUT_PORT_CENTER_Y + index * GRID_SIZE
    } else {
        position.y + base_y_offset + index * GRID_SIZE + radius
    };

    Some((center_x, center_y))
}

fn route_edge(
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    thickness: f32,
    edge_index: i32,
    snap: bool,
    mid_x_override: Option<f32>,
    segments: &mut Vec<EdgeSegmentVm>,
    corners: &mut Vec<EdgeCornerVm>,
) -> (f32, f32) {
    let min_dist = GRID_SIZE * 2.0;

    if to_x < from_x + min_dist {
        let mid_y = (from_y + to_y) / 2.0;
        let x_right = from_x + GRID_SIZE;
        let x_left = to_x - GRID_SIZE;

        let (mid_y, x_right, x_left) = if snap {
            (
                snap_to_grid_center(mid_y),
                snap_to_grid_center(x_right),
                snap_to_grid_center(x_left),
            )
        } else {
            (mid_y, x_right, x_left)
        };

        push_segment(segments, from_x, from_y, x_right, from_y, thickness, edge_index);
        push_segment(segments, x_right, from_y, x_right, mid_y, thickness, edge_index);
        push_segment(segments, x_right, mid_y, x_left, mid_y, thickness, edge_index);
        push_segment(segments, x_left, mid_y, x_left, to_y, thickness, edge_index);
        push_segment(segments, x_left, to_y, to_x, to_y, thickness, edge_index);

        corners.push(EdgeCornerVm { x: x_right, y: from_y, edge_index });
        corners.push(EdgeCornerVm { x: x_right, y: mid_y, edge_index });
        corners.push(EdgeCornerVm { x: x_left, y: mid_y, edge_index });
        corners.push(EdgeCornerVm { x: x_left, y: to_y, edge_index });

        ((x_right + x_left) / 2.0, mid_y)
    } else {
        let raw_mid = mid_x_override.unwrap_or((from_x + to_x) / 2.0);
        let mid_x = if snap {
            snap_to_grid_center(raw_mid)
        } else {
            raw_mid
        };

        push_segment(segments, from_x, from_y, mid_x, from_y, thickness, edge_index);
        push_segment(segments, mid_x, from_y, mid_x, to_y, thickness, edge_index);
        push_segment(segments, mid_x, to_y, to_x, to_y, thickness, edge_index);

        corners.push(EdgeCornerVm { x: mid_x, y: from_y, edge_index });
        corners.push(EdgeCornerVm { x: mid_x, y: to_y, edge_index });

        (mid_x, (from_y + to_y) / 2.0)
    }
}

/// Spacing between parallel vertical segments when edges overlap.
const EDGE_NUDGE_SPACING: f32 = GRID_SIZE * 0.5;
/// Edges whose raw mid_x falls within this distance are grouped together.
const EDGE_CHANNEL_THRESHOLD: f32 = GRID_SIZE;

pub(crate) fn build_edge_segments(
    graph: &NodeGraphDefinition,
    snap: bool,
) -> (Vec<EdgeSegmentVm>, Vec<EdgeCornerVm>, Vec<EdgeLabelVm>) {
    let mut segments = Vec::new();
    let mut corners = Vec::new();
    let mut labels = Vec::new();
    let thickness = GRID_SIZE * EDGE_THICKNESS_RATIO;
    let min_dist = GRID_SIZE * 2.0;

    // First pass: resolve coordinates and compute raw mid_x for each edge.
    struct EdgeInfo {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        raw_mid_x: f32,
        is_backward: bool,
        edge_idx: usize,
    }

    let mut infos: Vec<EdgeInfo> = Vec::new();
    for (idx, edge) in graph.edges.iter().enumerate() {
        let from_node = match graph.nodes.iter().find(|n| n.id == edge.from_node_id) {
            Some(node) => node,
            None => continue,
        };
        let to_node = match graph.nodes.iter().find(|n| n.id == edge.to_node_id) {
            Some(node) => node,
            None => continue,
        };

        let (from_x, from_y) = match get_port_center_for_node(from_node, &edge.from_port, false) {
            Some(pos) => pos,
            None => continue,
        };
        let (to_x, to_y) = match get_port_center_for_node(to_node, &edge.to_port, true) {
            Some(pos) => pos,
            None => continue,
        };

        let (from_x, from_y, to_x, to_y) = if snap {
            (
                snap_to_grid_center(from_x),
                snap_to_grid_center(from_y),
                snap_to_grid_center(to_x),
                snap_to_grid_center(to_y),
            )
        } else {
            (from_x, from_y, to_x, to_y)
        };

        let is_backward = to_x < from_x + min_dist;
        let raw_mid_x = (from_x + to_x) / 2.0;

        infos.push(EdgeInfo {
            from_x,
            from_y,
            to_x,
            to_y,
            raw_mid_x,
            is_backward,
            edge_idx: idx,
        });
    }

    // Second pass: group forward edges by similar mid_x and assign nudged offsets.
    let mut mid_x_overrides: Vec<Option<f32>> = vec![None; infos.len()];

    // Collect indices of forward (non-backward) edges for channel grouping.
    let mut forward_indices: Vec<usize> = infos
        .iter()
        .enumerate()
        .filter(|(_, info)| !info.is_backward)
        .map(|(i, _)| i)
        .collect();

    // Sort by raw_mid_x so we can group nearby ones.
    forward_indices.sort_by(|&a, &b| {
        infos[a]
            .raw_mid_x
            .partial_cmp(&infos[b].raw_mid_x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Group edges whose raw_mid_x is within the threshold.
    let mut i = 0;
    while i < forward_indices.len() {
        let group_start = i;
        let anchor = infos[forward_indices[i]].raw_mid_x;
        while i < forward_indices.len()
            && (infos[forward_indices[i]].raw_mid_x - anchor).abs() < EDGE_CHANNEL_THRESHOLD
        {
            i += 1;
        }
        let group_end = i;
        let group_size = group_end - group_start;
        if group_size > 1 {
            let center = anchor;
            let total_width = (group_size - 1) as f32 * EDGE_NUDGE_SPACING;
            for (j, &fi) in forward_indices[group_start..group_end].iter().enumerate() {
                mid_x_overrides[fi] =
                    Some(center - total_width / 2.0 + j as f32 * EDGE_NUDGE_SPACING);
            }
        }
    }

    // Third pass: route edges using the nudged mid_x values.
    for (info_idx, info) in infos.iter().enumerate() {
        let edge_index = info_idx as i32;

        let (label_x, label_y) = route_edge(
            info.from_x,
            info.from_y,
            info.to_x,
            info.to_y,
            thickness,
            edge_index,
            snap,
            mid_x_overrides[info_idx],
            &mut segments,
            &mut corners,
        );

        let edge = &graph.edges[info.edge_idx];
        let from_node = graph
            .nodes
            .iter()
            .find(|n| n.id == edge.from_node_id)
            .unwrap();
        let label_text = get_edge_data_type_label(graph, from_node, &edge.from_port)
            .unwrap_or_else(|| "Unknown".to_string());
        let label_width = (label_text.len() as f32 * 7.0).max(GRID_SIZE * 2.0);
        let label_height = GRID_SIZE * 0.8;

        labels.push(EdgeLabelVm {
            text: label_text.into(),
            x: label_x,
            y: label_y,
            width: label_width,
            height: label_height,
        });
    }

    (segments, corners, labels)
}

pub(crate) fn build_edges(
    graph: &NodeGraphDefinition,
    selection_state: &SelectionState,
    snap: bool,
) -> Vec<EdgeVm> {
    let selected_edge_from_node = &selection_state.selected_edge_from_node;
    let selected_edge_from_port = &selection_state.selected_edge_from_port;
    let selected_edge_to_node = &selection_state.selected_edge_to_node;
    let selected_edge_to_port = &selection_state.selected_edge_to_port;

    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = graph.nodes.iter().find(|n| n.id == edge.from_node_id)?;
            let to_node = graph.nodes.iter().find(|n| n.id == edge.to_node_id)?;

            let (from_x, from_y) = get_port_center_for_node(from_node, &edge.from_port, false)?;
            let (to_x, to_y) = get_port_center_for_node(to_node, &edge.to_port, true)?;

            let (from_x, from_y, to_x, to_y) = if snap {
                (
                    snap_to_grid_center(from_x),
                    snap_to_grid_center(from_y),
                    snap_to_grid_center(to_x),
                    snap_to_grid_center(to_y),
                )
            } else {
                (from_x, from_y, to_x, to_y)
            };

            let is_selected = !selected_edge_from_node.is_empty()
                && edge.from_node_id == selected_edge_from_node.as_str()
                && edge.from_port == selected_edge_from_port.as_str()
                && edge.to_node_id == selected_edge_to_node.as_str()
                && edge.to_port == selected_edge_to_port.as_str();

            Some(EdgeVm {
                from_node_id: edge.from_node_id.clone().into(),
                from_port: edge.from_port.clone().into(),
                to_node_id: edge.to_node_id.clone().into(),
                to_port: edge.to_port.clone().into(),
                from_x: from_x.into(),
                from_y: from_y.into(),
                to_x: to_x.into(),
                to_y: to_y.into(),
                is_selected,
            })
        })
        .collect()
}

fn push_segment(
    segments: &mut Vec<EdgeSegmentVm>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    thickness: f32,
    edge_index: i32,
) {
    if (x1 - x2).abs() < f32::EPSILON && (y1 - y2).abs() < f32::EPSILON {
        return;
    }

    let (x, y, width, height) = if (y1 - y2).abs() < f32::EPSILON {
        let min_x = x1.min(x2);
        let length = (x1 - x2).abs() + thickness;
        (min_x - thickness / 2.0, y1 - thickness / 2.0, length, thickness)
    } else {
        let min_y = y1.min(y2);
        let length = (y1 - y2).abs() + thickness;
        (x1 - thickness / 2.0, min_y - thickness / 2.0, thickness, length)
    };

    segments.push(EdgeSegmentVm {
        x,
        y,
        width,
        height,
        edge_index,
    });
}

fn get_edge_data_type_label(
    graph: &NodeGraphDefinition,
    node: &NodeDefinition,
    port_name: &str,
) -> Option<String> {
    Some(resolve_display_data_type(graph, node, port_name, false))
}

pub(crate) fn resolve_display_data_type(
    graph: &NodeGraphDefinition,
    node: &NodeDefinition,
    port_name: &str,
    is_input: bool,
) -> String {
    let mut visited = HashSet::new();
    resolve_display_data_type_inner(graph, node, port_name, is_input, &mut visited)
        .map(|data_type| data_type.to_string())
        .unwrap_or_else(|| "Unknown".to_string())
}

fn resolve_display_data_type_inner(
    graph: &NodeGraphDefinition,
    node: &NodeDefinition,
    port_name: &str,
    is_input: bool,
    visited: &mut HashSet<String>,
) -> Option<DataType> {
    let visit_key = format!("{}:{}:{}", node.id, if is_input { "in" } else { "out" }, port_name);
    if !visited.insert(visit_key) {
        return declared_port_type(node, port_name, is_input);
    }

    let declared = declared_port_type(node, port_name, is_input)?;
    if !matches!(declared, DataType::Any) {
        return Some(declared);
    }

    if is_input {
        if let Some(edge) = graph
            .edges
            .iter()
            .find(|edge| edge.to_node_id == node.id && edge.to_port == port_name)
        {
            let from_node = graph.nodes.iter().find(|candidate| candidate.id == edge.from_node_id)?;
            return resolve_display_data_type_inner(graph, from_node, &edge.from_port, false, visited)
                .or(Some(DataType::Any));
        }
        return Some(DataType::Any);
    }

    if node.node_type == "switch_gate" && port_name == "output" {
        return resolve_display_data_type_inner(graph, node, "input", true, visited)
            .or(Some(DataType::Any));
    }

    Some(DataType::Any)
}

fn declared_port_type(node: &NodeDefinition, port_name: &str, is_input: bool) -> Option<DataType> {
    let ports = if is_input { &node.input_ports } else { &node.output_ports };
    ports.iter().find(|p| p.name == port_name).map(|p| p.data_type.clone())
}

pub(crate) fn build_grid_lines(width: f32, height: f32, grid_size: f32) -> Vec<GridLineVm> {
    let mut lines = Vec::new();
    let mut x = 0.0;
    while x <= width {
        lines.push(GridLineVm { x1: x, y1: 0.0, x2: x, y2: height });
        x += grid_size;
    }

    let mut y = 0.0;
    while y <= height {
        lines.push(GridLineVm { x1: 0.0, y1: y, x2: width, y2: y });
        y += grid_size;
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::resolve_display_data_type;
    use crate::node::graph_io::{EdgeDefinition, NodeDefinition, NodeGraphDefinition};
    use crate::node::{DataType, Port};
    use std::collections::HashMap;

    fn node(id: &str, node_type: &str, input_ports: Vec<Port>, output_ports: Vec<Port>) -> NodeDefinition {
        NodeDefinition {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            node_type: node_type.to_string(),
            input_ports,
            output_ports,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
        }
    }

    #[test]
    fn switch_gate_displays_concrete_type_after_connection() {
        let source = node("source", "string_data", Vec::new(), vec![Port::new("value", DataType::String)]);
        let gate = node(
            "gate",
            "switch_gate",
            vec![Port::new("enabled", DataType::Boolean), Port::new("input", DataType::Any)],
            vec![Port::new("output", DataType::Any)],
        );

        let graph = NodeGraphDefinition {
            nodes: vec![source.clone(), gate.clone()],
            edges: vec![EdgeDefinition {
                from_node_id: "source".to_string(),
                from_port: "value".to_string(),
                to_node_id: "gate".to_string(),
                to_port: "input".to_string(),
            }],
            hyperparameters: Vec::new(),
            execution_results: HashMap::new(),
        };

        assert_eq!(resolve_display_data_type(&graph, &gate, "input", true), "String");
        assert_eq!(resolve_display_data_type(&graph, &gate, "output", false), "String");
    }

    #[test]
    fn switch_gate_keeps_any_when_unconnected() {
        let gate = node(
            "gate",
            "switch_gate",
            vec![Port::new("enabled", DataType::Boolean), Port::new("input", DataType::Any)],
            vec![Port::new("output", DataType::Any)],
        );

        let graph = NodeGraphDefinition {
            nodes: vec![gate.clone()],
            edges: Vec::new(),
            hyperparameters: Vec::new(),
            execution_results: HashMap::new(),
        };

        assert_eq!(resolve_display_data_type(&graph, &gate, "input", true), "Any");
        assert_eq!(resolve_display_data_type(&graph, &gate, "output", false), "Any");
    }
}
