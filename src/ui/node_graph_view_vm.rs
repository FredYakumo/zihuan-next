use std::collections::HashMap;

use slint::{ModelRc, VecModel};

use crate::node::graph_io::{ensure_positions, NodeDefinition, NodeGraphDefinition};
use crate::ui::graph_window::{HyperParameterVm, MessageItemVm, NodeGraphWindow, NodeTypeVm, NodeVm, PortVm};
use crate::ui::node_graph_view_geometry::{
    build_edge_segments, build_edges, build_grid_lines, node_dimensions, resolve_display_data_type,
    snap_to_grid, CANVAS_HEIGHT, CANVAS_WIDTH, GRID_SIZE,
};
use crate::ui::node_graph_view_inline::get_message_list_inline;
use crate::ui::node_render::{get_node_preview_text, inline_port_key, InlinePortValue};
use crate::ui::selection::SelectionState;

pub(crate) fn matches_node_type_search(node_type: &NodeTypeVm, search_text: &str) -> bool {
    if search_text.is_empty() {
        return true;
    }

    [
        node_type.type_id.as_str(),
        node_type.display_name.as_str(),
        node_type.category.as_str(),
        node_type.description.as_str(),
    ]
    .into_iter()
    .any(|field| field.to_lowercase().contains(search_text))
}

pub(crate) fn apply_graph_to_ui(
    ui: &NodeGraphWindow,
    graph: &NodeGraphDefinition,
    current_file: Option<String>,
    selection_state: &SelectionState,
    inline_inputs: &HashMap<String, InlinePortValue>,
    hyperparameter_values: &HashMap<String, serde_json::Value>,
) {
    apply_graph_to_ui_with_options(
        ui,
        graph,
        current_file,
        selection_state,
        inline_inputs,
        hyperparameter_values,
        true,
    );
}

pub(crate) fn apply_graph_to_ui_live(
    ui: &NodeGraphWindow,
    graph: &NodeGraphDefinition,
    current_file: Option<String>,
    selection_state: &SelectionState,
    inline_inputs: &HashMap<String, InlinePortValue>,
    hyperparameter_values: &HashMap<String, serde_json::Value>,
) {
    apply_graph_to_ui_with_options(
        ui,
        graph,
        current_file,
        selection_state,
        inline_inputs,
        hyperparameter_values,
        false,
    );
}

fn apply_graph_to_ui_with_options(
    ui: &NodeGraphWindow,
    graph: &NodeGraphDefinition,
    current_file: Option<String>,
    selection_state: &SelectionState,
    inline_inputs: &HashMap<String, InlinePortValue>,
    hyperparameter_values: &HashMap<String, serde_json::Value>,
    snap_positions: bool,
) {
    let mut graph = graph.clone();
    ensure_positions(&mut graph);

    if snap_positions {
        for node in &mut graph.nodes {
            if let Some(pos) = &mut node.position {
                pos.x = snap_to_grid(pos.x);
                pos.y = snap_to_grid(pos.y);
            }
        }
    }

    let nodes: Vec<NodeVm> = graph
        .nodes
        .iter()
        .map(|node| build_node_vm(node, &graph, selection_state, inline_inputs, snap_positions))
        .collect();

    let edges = build_edges(&graph, selection_state, snap_positions);
    let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&graph, snap_positions);

    let label = current_file.unwrap_or_else(|| "已加载 JSON".to_string());
    if snap_positions {
        let grid_lines = build_grid_lines(CANVAS_WIDTH, CANVAS_HEIGHT, GRID_SIZE);

        ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
        ui.set_edges(ModelRc::new(VecModel::from(edges)));
        ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
        ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
        ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
        ui.set_grid_lines(ModelRc::new(VecModel::from(grid_lines)));
        ui.set_current_file(label.into());

        let hyperparameter_vms: Vec<HyperParameterVm> = graph
            .hyperparameters
            .iter()
            .map(|hp| HyperParameterVm {
                name: hp.name.clone().into(),
                group: hp.group.clone().into(),
                data_type: hp.data_type.to_string().into(),
                value: hyperparameter_values
                    .get(&hp.name)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Number(n) => n.to_string(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
                    .into(),
                required: hp.required,
                description: hp.description.clone().unwrap_or_default().into(),
            })
            .collect();
        ui.set_hyperparameters(ModelRc::new(VecModel::from(hyperparameter_vms)));
    } else {
        update_nodes_model_in_place(ui, nodes);
        ui.set_edges(ModelRc::new(VecModel::from(edges)));
        ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
        ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
        ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
    }
}

fn update_nodes_model_in_place(ui: &NodeGraphWindow, nodes: Vec<NodeVm>) {
    use slint::Model;

    let model = ui.get_nodes();
    if model.row_count() == nodes.len() {
        for (index, node) in nodes.into_iter().enumerate() {
            model.set_row_data(index, node);
        }
    } else {
        ui.set_nodes(ModelRc::new(VecModel::from(nodes)));
    }
}

fn build_node_vm(
    node: &NodeDefinition,
    graph: &NodeGraphDefinition,
    selection_state: &SelectionState,
    inline_inputs: &HashMap<String, InlinePortValue>,
    snap_position: bool,
) -> NodeVm {
    let position = node.position.as_ref();
    let (node_width, node_height) = node_dimensions(node);
    let label = node.name.clone();
    let is_selected = selection_state.selected_node_ids.contains(&node.id);
    let preview_text = get_node_preview_text(&node.id, &node.node_type, graph, inline_inputs);

    let input_ports: Vec<PortVm> = node
        .input_ports
        .iter()
        .filter(|p| !(node.node_type == "brain" && p.name == "tools_config"))
        .map(|p| build_input_port_vm(node, p, graph, inline_inputs))
        .collect();

    let output_ports: Vec<PortVm> = node
        .output_ports
        .iter()
        .map(|p| PortVm {
            name: p.name.clone().into(),
            is_input: false,
            is_connected: graph.edges.iter().any(|e| e.from_node_id == node.id && e.from_port == p.name),
            is_required: false,
            has_value: false,
            data_type: resolve_display_data_type(graph, node, &p.name, false).into(),
            inline_text: "".into(),
            inline_bool: false,
            bound_hyperparameter: "".into(),
        })
        .collect();

    let string_data_text = if node.node_type == "string_data" {
        let key = inline_port_key(&node.id, "text");
        match inline_inputs.get(&key) {
            Some(InlinePortValue::Text(value)) => value.clone(),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let message_event_filter_type = if node.node_type == "message_event_type_filter" {
        let key = inline_port_key(&node.id, "filter_type");
        match inline_inputs.get(&key) {
            Some(InlinePortValue::Text(value)) => value.clone(),
            _ => "private".to_string(),
        }
    } else if node.node_type == "string_to_openai_message" || node.node_type == "as_system_openai_message" {
        let key = inline_port_key(&node.id, "role");
        match inline_inputs.get(&key) {
            Some(InlinePortValue::Text(value)) => value.clone(),
            _ => "system".to_string(),
        }
    } else {
        String::new()
    };

    let message_list = build_message_list_vm(node, graph, inline_inputs);

    let is_event_producer = crate::node::registry::NODE_REGISTRY.is_event_producer(&node.node_type);

    NodeVm {
        id: node.id.clone().into(),
        label: label.into(),
        preview_text: preview_text.into(),
        node_type: node.node_type.clone().into(),
        string_data_text: string_data_text.into(),
        message_event_filter_type: message_event_filter_type.into(),
        message_list: ModelRc::new(VecModel::from(message_list)),
        x: position
            .map(|p| if snap_position { snap_to_grid(p.x) } else { p.x })
            .unwrap_or(0.0),
        y: position
            .map(|p| if snap_position { snap_to_grid(p.y) } else { p.y })
            .unwrap_or(0.0),
        width: node_width,
        height: node_height,
        input_ports: ModelRc::new(VecModel::from(input_ports)),
        output_ports: ModelRc::new(VecModel::from(output_ports)),
        is_selected,
        has_error: node.has_error,
        has_cycle: node.has_cycle,
        is_event_producer,
    }
}

fn build_input_port_vm(
    node: &NodeDefinition,
    port: &crate::node::Port,
    graph: &NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
) -> PortVm {
    let bound_hp = node
        .port_bindings
        .get(&port.name)
        .cloned()
        .unwrap_or_default();
    let is_connected = graph.edges.iter().any(|e| e.to_node_id == node.id && e.to_port == port.name);
    let key = inline_port_key(&node.id, &port.name);
    let (inline_text, inline_bool, has_inline) = match &port.data_type {
        crate::node::DataType::Boolean => {
            let value = match inline_inputs.get(&key) {
                Some(InlinePortValue::Bool(v)) => *v,
                Some(InlinePortValue::Text(v)) => v.eq_ignore_ascii_case("true"),
                Some(InlinePortValue::Json(_)) => false,
                None => false,
            };
            (String::new(), value, true)
        }
        crate::node::DataType::String
        | crate::node::DataType::Integer
        | crate::node::DataType::Float
        | crate::node::DataType::Password => {
            let value = match inline_inputs.get(&key) {
                Some(InlinePortValue::Text(v)) => v.clone(),
                Some(InlinePortValue::Bool(v)) => v.to_string(),
                Some(InlinePortValue::Json(_)) => String::new(),
                None => String::new(),
            };
            let has_val = !value.is_empty();
            (value, false, has_val)
        }
        crate::node::DataType::Vec(inner)
            if matches!(
                inner.as_ref(),
                crate::node::DataType::OpenAIMessage | crate::node::DataType::QQMessage
            ) =>
        {
            let has_val = match inline_inputs.get(&key) {
                Some(InlinePortValue::Json(serde_json::Value::Array(arr))) => !arr.is_empty(),
                _ => false,
            };
            (String::new(), false, has_val)
        }
        _ => (String::new(), false, false),
    };

    PortVm {
        name: port.name.clone().into(),
        is_input: true,
        is_connected,
        is_required: port.required,
        has_value: has_inline || !bound_hp.is_empty(),
        data_type: resolve_display_data_type(graph, node, &port.name, true).into(),
        inline_text: inline_text.into(),
        inline_bool,
        bound_hyperparameter: bound_hp.into(),
    }
}

fn build_message_list_vm(
    node: &NodeDefinition,
    graph: &NodeGraphDefinition,
    inline_inputs: &HashMap<String, InlinePortValue>,
) -> Vec<MessageItemVm> {
    if node.node_type == "preview_message_list" {
        use crate::ui::node_render::preview_message_list::get_message_list_data;
        return get_message_list_data(&node.id, graph)
            .into_iter()
            .map(|msg| MessageItemVm {
                role: msg.role.into(),
                content: msg.content.into(),
            })
            .collect();
    }

    if node.node_type == "message_list_data" {
        return get_message_list_inline(inline_inputs, &node.id)
            .iter()
            .filter_map(|v| v.as_object())
            .map(|m| MessageItemVm {
                role: m.get("role").and_then(|v| v.as_str()).unwrap_or("user").to_string().into(),
                content: m.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string().into(),
            })
            .collect();
    }

    if node.node_type == "qq_message_list_data" {
        return get_message_list_inline(inline_inputs, &node.id)
            .iter()
            .filter_map(|v| v.as_object())
            .map(|m| {
                let msg_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("text").to_string();
                let data = m.get("data").and_then(|v| v.as_object());
                let content = data
                    .map(|d| match msg_type.as_str() {
                        "text" => d.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        "at" => d.get("target").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                        _ => d.get("id").map(|v| v.to_string()).unwrap_or_default(),
                    })
                    .unwrap_or_default();
                MessageItemVm {
                    role: msg_type.into(),
                    content: content.into(),
                }
            })
            .collect();
    }

    Vec::new()
}
