use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use zihuan_node::graph_io::{
    find_cycle_edge_keys, EdgeDefinition, NodeDefinition, NodeGraphDefinition,
};
use zihuan_node::function_graph::is_hidden_function_port;
use zihuan_node::DataType;
use crate::ui::graph_window::{EdgeCornerVm, EdgeLabelVm, EdgeSegmentVm, EdgeVm};
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
    matches!(
        node.node_type.as_str(),
        "message_list_data" | "qq_message_list_data"
    )
}

fn is_brain_node(node: &NodeDefinition) -> bool {
    matches!(node.node_type.as_str(), "brain" | "qq_message_agent")
}

fn is_visible_input_port(node_type: &str, port_name: &str) -> bool {
    if is_hidden_function_port(node_type, port_name) {
        return false;
    }

    !matches!(
        (node_type, port_name),
        ("brain", "tools_config")
            | ("brain", "shared_inputs")
            | ("qq_message_agent", "tools_config")
            | ("qq_message_agent", "shared_inputs")
            | ("message_event_type_filter", "filter_type")
            | ("send_qq_message_batches", "target_type")
            | ("string_to_openai_message", "role")
            | ("as_system_openai_message", "role")
            | ("format_string", "template")
            | ("set_variable", "variable_name")
            | ("set_variable", "variable_type")
            | ("json_extract", "fields_config")
    )
}

pub(crate) fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SIZE).round() * GRID_SIZE
}

pub(crate) fn snap_to_grid_center(value: f32) -> f32 {
    snap_to_grid(value - GRID_SIZE / 2.0) + GRID_SIZE / 2.0
}

pub(crate) fn node_dimensions(node: &NodeDefinition) -> (f32, f32) {
    let min_width = GRID_SIZE * NODE_WIDTH_CELLS;
    let visible_input_count = node
        .input_ports
        .iter()
        .filter(|port| is_visible_input_port(&node.node_type, &port.name))
        .count();
    let port_rows = visible_input_count.max(node.output_ports.len()) as f32;
    let default_min_height =
        GRID_SIZE * (NODE_MIN_ROWS.max(NODE_HEADER_ROWS + port_rows) + NODE_PADDING_BOTTOM);
    let min_height = if is_list_data_node(node) {
        default_min_height.max(LIST_NODE_MIN_HEIGHT)
    } else if is_brain_node(node) {
        default_min_height.max(BRAIN_NODE_MIN_HEIGHT)
    } else if node.node_type == "set_variable" {
        default_min_height + GRID_SIZE
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

    let ports = if is_input {
        node.input_ports
            .iter()
            .filter(|port| is_visible_input_port(&node.node_type, &port.name))
            .collect::<Vec<_>>()
    } else {
        node.output_ports.iter().collect::<Vec<_>>()
    };
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
    push_segment(
        segments,
        from_x,
        from_y,
        source_channel_x,
        from_y,
        thickness,
        edge_index,
    );
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
    push_segment(
        segments,
        target_channel_x,
        to_y,
        to_x,
        to_y,
        thickness,
        edge_index,
    );

    corners.push(EdgeCornerVm {
        x: source_channel_x,
        y: from_y,
        edge_index,
    });
    corners.push(EdgeCornerVm {
        x: source_channel_x,
        y: lane_y,
        edge_index,
    });
    corners.push(EdgeCornerVm {
        x: target_channel_x,
        y: lane_y,
        edge_index,
    });
    corners.push(EdgeCornerVm {
        x: target_channel_x,
        y: to_y,
        edge_index,
    });

    ((source_channel_x + target_channel_x) / 2.0, lane_y)
}

const EDGE_SOURCE_CHANNEL_BASE: f32 = GRID_SIZE * 1.5;
const EDGE_SOURCE_CHANNEL_SPACING: f32 = GRID_SIZE * 0.8;
const EDGE_TARGET_CHANNEL_BASE: f32 = GRID_SIZE * 1.5;
const EDGE_TARGET_CHANNEL_SPACING: f32 = GRID_SIZE * 0.8;
const EDGE_LANE_SPACING: f32 = GRID_SIZE * 1.2;
const EDGE_LANE_THRESHOLD: f32 = GRID_SIZE * 2.0;

/// Axis-aligned bounding box of an on-canvas node (world-space).
struct NodeBox {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

fn collect_node_boxes(graph: &NodeGraphDefinition) -> Vec<NodeBox> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let pos = node.position.as_ref()?;
            let (w, h) = node_dimensions(node);
            Some(NodeBox {
                left: pos.x,
                top: pos.y,
                right: pos.x + w,
                bottom: pos.y + h,
            })
        })
        .collect()
}

/// Return a `candidate_y` adjusted so the horizontal wire segment running
/// between `span_min_x` and `span_max_x` does not pass through any node card.
/// Uses a greedy nearest-gap strategy: route above or below whichever is closer.
fn find_obstacle_free_lane_y(
    candidate_y: f32,
    span_min_x: f32,
    span_max_x: f32,
    node_boxes: &[NodeBox],
) -> f32 {
    const MARGIN: f32 = GRID_SIZE * 0.8;
    // Boxes whose X range intersects [span_min_x, span_max_x] AND
    // whose top/bottom bracket candidate_y (with margin).
    let blockers: Vec<&NodeBox> = node_boxes
        .iter()
        .filter(|b| {
            b.left < span_max_x
                && b.right > span_min_x
                && (b.top - MARGIN) < candidate_y
                && (b.bottom + MARGIN) > candidate_y
        })
        .collect();

    if blockers.is_empty() {
        return candidate_y;
    }

    let above_y = blockers
        .iter()
        .map(|b| b.top)
        .fold(f32::INFINITY, f32::min)
        - MARGIN;
    let below_y = blockers
        .iter()
        .map(|b| b.bottom)
        .fold(f32::NEG_INFINITY, f32::max)
        + MARGIN;

    if (above_y - candidate_y).abs() <= (below_y - candidate_y).abs() {
        above_y
    } else {
        below_y
    }
}

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
    let node_boxes = collect_node_boxes(graph);

    let node_map: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();

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
        let from_node = match node_map.get(edge.from_node_id.as_str()) {
            Some(&i) => &graph.nodes[i],
            None => continue,
        };
        let to_node = match node_map.get(edge.to_node_id.as_str()) {
            Some(&i) => &graph.nodes[i],
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
            infos[info_idx].source_channel_x = infos[info_idx].from_x
                + EDGE_SOURCE_CHANNEL_BASE
                + order as f32 * EDGE_SOURCE_CHANNEL_SPACING;
            infos[info_idx].candidate_lane_y +=
                centered_group_offset(order, members.len()) * EDGE_LANE_SPACING * 0.65;
        }
    }

    for members in target_groups.values_mut() {
        members.sort_by(|&a, &b| cmp_f32(infos[a].to_y, infos[b].to_y));
        for (order, &info_idx) in members.iter().enumerate() {
            infos[info_idx].target_channel_x = infos[info_idx].to_x
                - EDGE_TARGET_CHANNEL_BASE
                - order as f32 * EDGE_TARGET_CHANNEL_SPACING;
            infos[info_idx].candidate_lane_y +=
                centered_group_offset(order, members.len()) * EDGE_LANE_SPACING * 0.35;
        }
    }

    for info in &mut infos {
        info.span_min_x = info.source_channel_x.min(info.target_channel_x);
        info.span_max_x = info.source_channel_x.max(info.target_channel_x);
    }

    // Avoidance pass 1: shift each edge's candidate lane out of any node body
    // it would cross, before Union-Find groups them.  This ensures the grouping
    // centers reflect already-displaced lanes and reduces second-order collisions.
    for info in &mut infos {
        info.candidate_lane_y = find_obstacle_free_lane_y(
            info.candidate_lane_y,
            info.span_min_x,
            info.span_max_x,
            &node_boxes,
        );
    }

    let n = infos.len();
    let mut lane_y_overrides: Vec<Option<f32>> = vec![None; n];

    if n > 1 {
        let mut parent: Vec<usize> = (0..n).collect();

        for a in 0..n {
            for b in (a + 1)..n {
                let x_overlap = infos[a].span_min_x < infos[b].span_max_x
                    && infos[b].span_min_x < infos[a].span_max_x;
                let lane_close = (infos[a].candidate_lane_y - infos[b].candidate_lane_y).abs()
                    < EDGE_LANE_THRESHOLD;

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

            // Sort by the natural mid-Y of each wire so lanes are ordered in
            // the same direction the wires travel – this minimises crossings
            // within a conflict bundle.
            members.sort_by(|&a, &b| {
                let mid_a = (infos[a].from_y + infos[a].to_y) / 2.0;
                let mid_b = (infos[b].from_y + infos[b].to_y) / 2.0;
                cmp_f32(mid_a, mid_b)
                    .then_with(|| cmp_f32(infos[a].from_y, infos[b].from_y))
                    .then_with(|| cmp_f32(infos[a].candidate_lane_y, infos[b].candidate_lane_y))
            });

            let center = members
                .iter()
                .map(|&idx| infos[idx].candidate_lane_y)
                .sum::<f32>()
                / members.len() as f32;
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
        .map(|(info_idx, info)| {
            let raw_lane_y =
                lane_y_overrides[info_idx].unwrap_or(info.candidate_lane_y);
            // Avoidance pass 2: re-check after Union-Find may have shifted lanes;
            // ensures conflict-resolved lanes don't land inside a node body.
            let avoided_y = find_obstacle_free_lane_y(
                raw_lane_y,
                info.span_min_x,
                info.span_max_x,
                &node_boxes,
            );
            EdgeRoutePlan {
                from_x: info.from_x,
                from_y: info.from_y,
                to_x: info.to_x,
                to_y: info.to_y,
                source_channel_x: info.source_channel_x,
                target_channel_x: info.target_channel_x,
                lane_y: if snap {
                    snap_to_grid_center(avoided_y)
                } else {
                    avoided_y
                },
                edge_idx: info.edge_idx,
            }
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

    let node_map: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();

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
        let from_node = &graph.nodes[node_map[edge.from_node_id.as_str()]];
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

/// Fast drag-time edge position updater: translates existing edge geometry in-place
/// by applying node position deltas. Avoids recalculating edge routing entirely.
///
/// `node_deltas` maps node_id → (dx, dy) for all dragged nodes.
/// For edges where both endpoints have a delta, translate the entire edge geometry
/// by that delta. For edges where only one endpoint moved, translate both endpoints
/// by the weighted delta — this is a rough approximation that avoids full re-routing.
pub(crate) fn translate_edge_geometry(
    ui: &crate::ui::graph_window::NodeGraphWindow,
    node_deltas: &HashMap<String, (f32, f32)>,
) {
    use slint::Model;

    if node_deltas.is_empty() {
        return;
    }

    let edges_model = ui.get_edges();
    let segments_model = ui.get_edge_segments();
    let corners_model = ui.get_edge_corners();
    let labels_model = ui.get_edge_labels();

    // Build per-edge delta: for each edge, compute what dx/dy to apply.
    // If both from/to nodes are dragged, they share the same delta (rigid move).
    // If only one endpoint is dragged, we still translate the whole edge by
    // half the delta for each endpoint — but for simplicity during drag,
    // we compute from_delta and to_delta separately.
    let edge_count = edges_model.row_count();
    let mut edge_from_delta: Vec<(f32, f32)> = Vec::with_capacity(edge_count);
    let mut edge_to_delta: Vec<(f32, f32)> = Vec::with_capacity(edge_count);

    for i in 0..edge_count {
        let edge = edges_model.row_data(i).unwrap();
        let fd = node_deltas
            .get(edge.from_node_id.as_str())
            .copied()
            .unwrap_or((0.0, 0.0));
        let td = node_deltas
            .get(edge.to_node_id.as_str())
            .copied()
            .unwrap_or((0.0, 0.0));
        edge_from_delta.push(fd);
        edge_to_delta.push(td);

        // Update the edge endpoint positions
        if fd != (0.0, 0.0) || td != (0.0, 0.0) {
            let mut e = edge;
            e.from_x += fd.0;
            e.from_y += fd.1;
            e.to_x += td.0;
            e.to_y += td.1;
            edges_model.set_row_data(i, e);
        }
    }

    // Translate edge segments: each segment belongs to an edge (edge_index).
    // Use the average of from/to deltas as the segment translation.
    let seg_count = segments_model.row_count();
    for i in 0..seg_count {
        let seg = segments_model.row_data(i).unwrap();
        let ei = seg.edge_index as usize;
        if ei < edge_count {
            let (fdx, fdy) = edge_from_delta[ei];
            let (tdx, tdy) = edge_to_delta[ei];
            let dx = (fdx + tdx) / 2.0;
            let dy = (fdy + tdy) / 2.0;
            if dx != 0.0 || dy != 0.0 {
                let mut s = seg;
                s.x += dx;
                s.y += dy;
                segments_model.set_row_data(i, s);
            }
        }
    }

    // Translate edge corners
    let corner_count = corners_model.row_count();
    for i in 0..corner_count {
        let corner = corners_model.row_data(i).unwrap();
        let ei = corner.edge_index as usize;
        if ei < edge_count {
            let (fdx, fdy) = edge_from_delta[ei];
            let (tdx, tdy) = edge_to_delta[ei];
            let dx = (fdx + tdx) / 2.0;
            let dy = (fdy + tdy) / 2.0;
            if dx != 0.0 || dy != 0.0 {
                let mut c = corner;
                c.x += dx;
                c.y += dy;
                corners_model.set_row_data(i, c);
            }
        }
    }

    // Translate edge labels — labels don't have edge_index, so we match positionally.
    // Labels are generated 1:1 with edge route plans (same order as edges model).
    let label_count = labels_model.row_count();
    for i in 0..label_count {
        if i >= edge_count {
            break;
        }
        let (fdx, fdy) = edge_from_delta[i];
        let (tdx, tdy) = edge_to_delta[i];
        let dx = (fdx + tdx) / 2.0;
        let dy = (fdy + tdy) / 2.0;
        if dx != 0.0 || dy != 0.0 {
            let label = labels_model.row_data(i).unwrap();
            let mut l = label;
            l.x += dx;
            l.y += dy;
            labels_model.set_row_data(i, l);
        }
    }
}

const N_EDGE_COLORS: usize = 8;

fn edge_color(edge: &EdgeDefinition) -> slint::Color {
    let key = format!(
        "{}{}{}{}",
        edge.from_node_id, edge.from_port, edge.to_node_id, edge.to_port
    );
    let hash = key.bytes().fold(0usize, |acc, b| {
        acc.wrapping_mul(31).wrapping_add(b as usize)
    });
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

fn edge_has_cycle(
    cycle_edge_keys: &HashSet<(String, String, String, String)>,
    edge: &EdgeDefinition,
) -> bool {
    cycle_edge_keys.contains(&(
        edge.from_node_id.clone(),
        edge.from_port.clone(),
        edge.to_node_id.clone(),
        edge.to_port.clone(),
    ))
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
    let cycle_edge_keys = if graph.nodes.iter().any(|node| node.has_cycle) {
        find_cycle_edge_keys(graph)
    } else {
        HashSet::new()
    };

    let node_map: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();

    let error_nodes: HashSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.has_error)
        .map(|n| n.id.as_str())
        .collect();

    graph
        .edges
        .iter()
        .filter_map(|edge| {
            let from_node = &graph.nodes[*node_map.get(edge.from_node_id.as_str())?];
            let to_node = &graph.nodes[*node_map.get(edge.to_node_id.as_str())?];

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
            let has_error = error_nodes.contains(edge.from_node_id.as_str())
                || error_nodes.contains(edge.to_node_id.as_str());
            let has_cycle = edge_has_cycle(&cycle_edge_keys, edge);

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
                has_error,
                has_cycle,
                color: if has_error {
                    slint::Color::from_rgb_u8(0xdc, 0x35, 0x45)
                } else if has_cycle {
                    slint::Color::from_rgb_u8(0xf2, 0xc9, 0x4c)
                } else {
                    edge_color(edge)
                },
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
        (
            min_x - thickness / 2.0,
            y1 - thickness / 2.0,
            length,
            thickness,
        )
    } else {
        let min_y = y1.min(y2);
        let length = (y1 - y2).abs() + thickness;
        (
            x1 - thickness / 2.0,
            min_y - thickness / 2.0,
            thickness,
            length,
        )
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
    let visit_key = format!(
        "{}:{}:{}",
        node.id,
        if is_input { "in" } else { "out" },
        port_name
    );
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
            let from_node = graph
                .nodes
                .iter()
                .find(|candidate| candidate.id == edge.from_node_id)?;
            return resolve_display_data_type_inner(
                graph,
                from_node,
                &edge.from_port,
                false,
                visited,
            )
            .or(Some(DataType::Any));
        }
        return Some(DataType::Any);
    }

    // Vec-transformation nodes: infer concrete output types from their Any-typed inputs.
    match (node.node_type.as_str(), port_name) {
        ("stack", "array") => {
            // element: Any  →  array: Vec(element_type)
            if let Some(inner) =
                resolve_display_data_type_inner(graph, node, "element", true, visited)
            {
                let inner = match inner {
                    DataType::Any => return Some(DataType::Any),
                    t => t,
                };
                return Some(DataType::Vec(Box::new(inner)));
            }
        }
        ("array_get", "element") => {
            // array: Vec(T)  →  element: T
            if let Some(vec_type) =
                resolve_display_data_type_inner(graph, node, "array", true, visited)
            {
                if let DataType::Vec(inner) = vec_type {
                    return Some(*inner);
                }
            }
        }
        ("push_back_vec", "result") => {
            // vec: Vec(T), element: Any  →  result: Vec(T)
            if let Some(t) =
                resolve_display_data_type_inner(graph, node, "vec", true, visited)
            {
                if !matches!(t, DataType::Any) {
                    return Some(t);
                }
            }
        }
        ("concat_vec", "vec") => {
            // vec1: Vec(T), vec2: Vec(T)  →  vec: Vec(T)
            for input_name in ["vec1", "vec2"] {
                if let Some(t) =
                    resolve_display_data_type_inner(graph, node, input_name, true, visited)
                {
                    if !matches!(t, DataType::Any) {
                        return Some(t);
                    }
                }
            }
        }
        _ => {}
    }

    // Generic passthrough: for Any-typed output ports, trace back through all Any-typed
    // input ports and return the first concrete type found. This covers switch_gate,
    // and_then, boolean_branch, conditional, conditional_router, loop, loop_break, etc.
    let any_inputs: Vec<String> = node
        .input_ports
        .iter()
        .filter(|p| matches!(p.data_type, DataType::Any))
        .map(|p| p.name.clone())
        .collect();
    for input_name in any_inputs {
        if let Some(t) =
            resolve_display_data_type_inner(graph, node, &input_name, true, visited)
        {
            if !matches!(t, DataType::Any) {
                return Some(t);
            }
        }
    }

    Some(DataType::Any)
}

fn declared_port_type(node: &NodeDefinition, port_name: &str, is_input: bool) -> Option<DataType> {
    let ports = if is_input {
        &node.input_ports
    } else {
        &node.output_ports
    };
    ports
        .iter()
        .find(|p| p.name == port_name)
        .map(|p| p.data_type.clone())
}

pub(crate) fn build_grid_commands(width: f32, height: f32, grid_size: f32) -> String {
    use std::fmt::Write;
    let num_vertical = (width / grid_size) as usize + 1;
    let num_horizontal = (height / grid_size) as usize + 1;
    // Each line takes ~22 chars: "M 0000 0000 L 0000 0000 "
    let mut cmds = String::with_capacity(24 * (num_vertical + num_horizontal));

    let mut x = 0.0f32;
    while x <= width {
        let _ = write!(cmds, "M {} 0 L {} {} ", x, x, height);
        x += grid_size;
    }

    let mut y = 0.0f32;
    while y <= height {
        let _ = write!(cmds, "M 0 {} L {} {} ", y, width, y);
        y += grid_size;
    }

    cmds
}

