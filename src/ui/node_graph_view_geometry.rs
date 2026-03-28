use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::node::graph_io::{EdgeDefinition, NodeDefinition, NodeGraphDefinition};
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
    source_channel_x: f32,
    target_channel_x: f32,
    lane_y: f32,
    thickness: f32,
    edge_index: i32,
    segments: &mut Vec<EdgeSegmentVm>,
    corners: &mut Vec<EdgeCornerVm>,
) -> (f32, f32) {
    push_segment(segments, from_x, from_y, source_channel_x, from_y, thickness, edge_index);
    push_segment(
        segments,
        source_channel_x,
        from_y,
        source_channel_x,
        lane_y,
        thickness,
        edge_index,
    );
    push_segment(
        segments,
        source_channel_x,
        lane_y,
        target_channel_x,
        lane_y,
        thickness,
        edge_index,
    );
    push_segment(
        segments,
        target_channel_x,
        lane_y,
        target_channel_x,
        to_y,
        thickness,
        edge_index,
    );
    push_segment(segments, target_channel_x, to_y, to_x, to_y, thickness, edge_index);

    corners.push(EdgeCornerVm { x: source_channel_x, y: from_y, edge_index });
    corners.push(EdgeCornerVm { x: source_channel_x, y: lane_y, edge_index });
    corners.push(EdgeCornerVm { x: target_channel_x, y: lane_y, edge_index });
    corners.push(EdgeCornerVm { x: target_channel_x, y: to_y, edge_index });

    ((source_channel_x + target_channel_x) / 2.0, lane_y)
}

const EDGE_SOURCE_CHANNEL_BASE: f32 = GRID_SIZE * 1.0;
const EDGE_SOURCE_CHANNEL_SPACING: f32 = GRID_SIZE * 0.8;
const EDGE_TARGET_CHANNEL_BASE: f32 = GRID_SIZE * 1.0;
const EDGE_TARGET_CHANNEL_SPACING: f32 = GRID_SIZE * 0.8;
const EDGE_LANE_SPACING: f32 = GRID_SIZE * 1.0;
const EDGE_LANE_THRESHOLD: f32 = GRID_SIZE * 2.0;

#[derive(Clone)]
struct EdgeRoutePlan {
    from_x: f32,
    from_y: f32,
    to_x: f32,
    to_y: f32,
    source_channel_x: f32,
    target_channel_x: f32,
    lane_y: f32,
    edge_idx: usize,
}

fn cmp_f32(a: f32, b: f32) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn centered_group_offset(index: usize, len: usize) -> f32 {
    index as f32 - (len.saturating_sub(1) as f32 / 2.0)
}

fn build_edge_route_plans(graph: &NodeGraphDefinition, snap: bool) -> Vec<EdgeRoutePlan> {
    #[derive(Clone)]
    struct EdgeInfo {
        from_x: f32,
        from_y: f32,
        to_x: f32,
        to_y: f32,
        candidate_lane_y: f32,
        source_channel_x: f32,
        target_channel_x: f32,
        span_min_x: f32,
        span_max_x: f32,
        source_key: (String, String),
        target_node_id: String,
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

        infos.push(EdgeInfo {
            from_x,
            from_y,
            to_x,
            to_y,
            candidate_lane_y: (from_y + to_y) / 2.0,
            source_channel_x: from_x + EDGE_SOURCE_CHANNEL_BASE,
            target_channel_x: to_x - EDGE_TARGET_CHANNEL_BASE,
            span_min_x: 0.0,
            span_max_x: 0.0,
            source_key: (edge.from_node_id.clone(), edge.from_port.clone()),
            target_node_id: edge.to_node_id.clone(),
            edge_idx: idx,
        });
    }

    let mut source_groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut target_groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, info) in infos.iter().enumerate() {
        source_groups
            .entry(info.source_key.clone())
            .or_default()
            .push(idx);
        target_groups
            .entry(info.target_node_id.clone())
            .or_default()
            .push(idx);
    }

    for members in source_groups.values_mut() {
        members.sort_by(|&a, &b| cmp_f32(infos[a].to_y, infos[b].to_y));
        for (order, &info_idx) in members.iter().enumerate() {
            infos[info_idx].source_channel_x =
                infos[info_idx].from_x + EDGE_SOURCE_CHANNEL_BASE + order as f32 * EDGE_SOURCE_CHANNEL_SPACING;
            infos[info_idx].candidate_lane_y +=
                centered_group_offset(order, members.len()) * EDGE_LANE_SPACING * 0.65;
        }
    }

    for members in target_groups.values_mut() {
        members.sort_by(|&a, &b| cmp_f32(infos[a].to_y, infos[b].to_y));
        for (order, &info_idx) in members.iter().enumerate() {
            infos[info_idx].target_channel_x =
                infos[info_idx].to_x - EDGE_TARGET_CHANNEL_BASE - order as f32 * EDGE_TARGET_CHANNEL_SPACING;
            infos[info_idx].candidate_lane_y +=
                centered_group_offset(order, members.len()) * EDGE_LANE_SPACING * 0.35;
        }
    }

    for info in &mut infos {
        info.span_min_x = info.source_channel_x.min(info.target_channel_x);
        info.span_max_x = info.source_channel_x.max(info.target_channel_x);
    }

    let n = infos.len();
    let mut lane_y_overrides: Vec<Option<f32>> = vec![None; n];

    if n > 1 {
        let mut parent: Vec<usize> = (0..n).collect();

        for a in 0..n {
            for b in (a + 1)..n {
                let x_overlap =
                    infos[a].span_min_x < infos[b].span_max_x && infos[b].span_min_x < infos[a].span_max_x;
                let lane_close = (infos[a].candidate_lane_y - infos[b].candidate_lane_y).abs() < EDGE_LANE_THRESHOLD;

                if x_overlap && lane_close {
                    let mut ra = a;
                    while parent[ra] != ra {
                        ra = parent[ra];
                    }
                    let mut rb = b;
                    while parent[rb] != rb {
                        rb = parent[rb];
                    }
                    if ra != rb {
                        parent[rb] = ra;
                    }
                }
            }
        }

        let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
        for idx in 0..n {
            let mut root = idx;
            while parent[root] != root {
                root = parent[root];
            }
            groups.entry(root).or_default().push(idx);
        }

        for members in groups.values_mut() {
            if members.len() <= 1 {
                continue;
            }

            members.sort_by(|&a, &b| {
                cmp_f32(infos[a].to_y, infos[b].to_y)
                    .then_with(|| cmp_f32(infos[a].from_y, infos[b].from_y))
                    .then_with(|| cmp_f32(infos[a].candidate_lane_y, infos[b].candidate_lane_y))
            });

            let center = members.iter().map(|&idx| infos[idx].candidate_lane_y).sum::<f32>() / members.len() as f32;
            let total_height = (members.len() - 1) as f32 * EDGE_LANE_SPACING;

            for (order, &info_idx) in members.iter().enumerate() {
                lane_y_overrides[info_idx] =
                    Some(center - total_height / 2.0 + order as f32 * EDGE_LANE_SPACING);
            }
        }
    }

    infos
        .into_iter()
        .enumerate()
        .map(|(info_idx, info)| EdgeRoutePlan {
            from_x: info.from_x,
            from_y: info.from_y,
            to_x: info.to_x,
            to_y: info.to_y,
            source_channel_x: info.source_channel_x,
            target_channel_x: info.target_channel_x,
            lane_y: if snap {
                snap_to_grid_center(lane_y_overrides[info_idx].unwrap_or(info.candidate_lane_y))
            } else {
                lane_y_overrides[info_idx].unwrap_or(info.candidate_lane_y)
            },
            edge_idx: info.edge_idx,
        })
        .collect()
}

pub(crate) fn build_edge_segments(
    graph: &NodeGraphDefinition,
    snap: bool,
) -> (Vec<EdgeSegmentVm>, Vec<EdgeCornerVm>, Vec<EdgeLabelVm>) {
    let mut segments = Vec::new();
    let mut corners = Vec::new();
    let mut labels = Vec::new();
    let thickness = GRID_SIZE * EDGE_THICKNESS_RATIO;
    let plans = build_edge_route_plans(graph, snap);

    for (plan_idx, plan) in plans.iter().enumerate() {
        let edge_index = plan_idx as i32;

        let (label_x, label_y) = route_edge(
            plan.from_x,
            plan.from_y,
            plan.to_x,
            plan.to_y,
            plan.source_channel_x,
            plan.target_channel_x,
            plan.lane_y,
            thickness,
            edge_index,
            &mut segments,
            &mut corners,
        );

        let edge = &graph.edges[plan.edge_idx];
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

const N_EDGE_COLORS: usize = 8;

fn edge_color(edge: &EdgeDefinition) -> slint::Color {
    let key = format!(
        "{}{}{}{}",
        edge.from_node_id, edge.from_port, edge.to_node_id, edge.to_port
    );
    let hash = key
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_mul(31).wrapping_add(b as usize));
    match hash % N_EDGE_COLORS {
        0 => slint::Color::from_rgb_u8(0xaa, 0xaa, 0xaa),
        1 => slint::Color::from_rgb_u8(0x5b, 0x9b, 0xd5),
        2 => slint::Color::from_rgb_u8(0xed, 0x7d, 0x31),
        3 => slint::Color::from_rgb_u8(0xa9, 0xd1, 0x8e),
        4 => slint::Color::from_rgb_u8(0xff, 0x7e, 0xb3),
        5 => slint::Color::from_rgb_u8(0xc5, 0xa3, 0xd5),
        6 => slint::Color::from_rgb_u8(0x4e, 0xcd, 0xc4),
        7 => slint::Color::from_rgb_u8(0xff, 0xd9, 0x3d),
        _ => unreachable!(),
    }
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
                color: edge_color(edge),
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
    use super::{build_edge_route_plans, resolve_display_data_type, GRID_SIZE};
    use crate::node::graph_io::{EdgeDefinition, GraphPosition, NodeDefinition, NodeGraphDefinition};
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
            dynamic_input_ports: false,
            dynamic_output_ports: false,
            position: None,
            size: None,
            inline_values: HashMap::new(),
            port_bindings: HashMap::new(),
            has_error: false,
        }
    }

    fn node_at(
        id: &str,
        node_type: &str,
        x: f32,
        y: f32,
        input_ports: Vec<Port>,
        output_ports: Vec<Port>,
    ) -> NodeDefinition {
        let mut node = node(id, node_type, input_ports, output_ports);
        node.position = Some(GraphPosition { x, y });
        node
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
            hyperparameter_groups: Vec::new(),
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
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            execution_results: HashMap::new(),
        };

        assert_eq!(resolve_display_data_type(&graph, &gate, "input", true), "Any");
        assert_eq!(resolve_display_data_type(&graph, &gate, "output", false), "Any");
    }

    #[test]
    fn same_output_port_edges_get_separate_source_channels_and_lanes() {
        let source = node_at(
            "source",
            "string_data",
            0.0,
            0.0,
            Vec::new(),
            vec![Port::new("value", DataType::String)],
        );
        let upper_target = node_at(
            "upper",
            "preview_string",
            420.0,
            -80.0,
            vec![Port::new("text", DataType::String)],
            Vec::new(),
        );
        let lower_target = node_at(
            "lower",
            "preview_string",
            420.0,
            180.0,
            vec![Port::new("text", DataType::String)],
            Vec::new(),
        );

        let graph = NodeGraphDefinition {
            nodes: vec![source, upper_target, lower_target],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "source".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "upper".to_string(),
                    to_port: "text".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "source".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "lower".to_string(),
                    to_port: "text".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            execution_results: HashMap::new(),
        };

        let plans = build_edge_route_plans(&graph, true);
        assert_eq!(plans.len(), 2);
        assert_ne!(plans[0].source_channel_x, plans[1].source_channel_x);
        assert_ne!(plans[0].lane_y, plans[1].lane_y);
    }

    #[test]
    fn same_target_node_edges_get_separate_target_channels() {
        let left_source = node_at(
            "left",
            "string_data",
            0.0,
            0.0,
            Vec::new(),
            vec![Port::new("value", DataType::String)],
        );
        let lower_source = node_at(
            "lower",
            "string_data",
            0.0,
            220.0,
            Vec::new(),
            vec![Port::new("value", DataType::String)],
        );
        let target = node_at(
            "target",
            "format_string",
            460.0,
            80.0,
            vec![
                Port::new("first", DataType::String),
                Port::new("second", DataType::String),
            ],
            vec![Port::new("output", DataType::String)],
        );

        let graph = NodeGraphDefinition {
            nodes: vec![left_source, lower_source, target],
            edges: vec![
                EdgeDefinition {
                    from_node_id: "left".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "target".to_string(),
                    to_port: "first".to_string(),
                },
                EdgeDefinition {
                    from_node_id: "lower".to_string(),
                    from_port: "value".to_string(),
                    to_node_id: "target".to_string(),
                    to_port: "second".to_string(),
                },
            ],
            hyperparameter_groups: Vec::new(),
            hyperparameters: Vec::new(),
            execution_results: HashMap::new(),
        };

        let plans = build_edge_route_plans(&graph, true);
        assert_eq!(plans.len(), 2);
        assert_ne!(plans[0].target_channel_x, plans[1].target_channel_x);
        assert!((plans[0].target_channel_x - plans[1].target_channel_x).abs() >= GRID_SIZE * 0.5);
    }
}
