use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::node::graph_io::ensure_positions;
use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{
    refresh_active_tab_ui, tab_display_title, update_tabs_ui, GraphTabState,
};
use crate::ui::node_graph_view_geometry::{
    build_edge_segments, get_port_center, node_dimensions, snap_to_grid,
    snap_to_grid_center, GRID_SIZE, NODE_HEADER_ROWS, NODE_MIN_ROWS, NODE_PADDING_BOTTOM,
    NODE_WIDTH_CELLS,
};
use crate::ui::node_graph_view_vm::apply_graph_to_ui;
use crate::ui::selection::BoxSelection;

pub(crate) fn bind_canvas_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_moved(move |node_id: SharedString, x: f32, y: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                if let Some(pos) = &mut node.position {
                    pos.x = x;
                    pos.y = y;
                } else {
                    node.position = Some(crate::node::graph_io::GraphPosition { x, y });
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&tab.graph, false);

                ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
                ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
                ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_resized(move |node_id: SharedString, width: f32, height: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                node.size = Some(crate::node::graph_io::GraphSize { width, height });
            }

            if let Some(ui) = ui_handle.upgrade() {
                let (edge_segments, edge_corners, edge_labels) = build_edge_segments(&tab.graph, false);

                ui.set_edge_segments(ModelRc::new(VecModel::from(edge_segments)));
                ui.set_edge_corners(ModelRc::new(VecModel::from(edge_corners)));
                ui.set_edge_labels(ModelRc::new(VecModel::from(edge_labels)));
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_move_finished(move |node_id: SharedString, x: f32, y: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let snapped_x = snap_to_grid(x);
            let snapped_y = snap_to_grid(y);
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                if let Some(pos) = &mut node.position {
                    pos.x = snapped_x;
                    pos.y = snapped_y;
                } else {
                    node.position = Some(crate::node::graph_io::GraphPosition {
                        x: snapped_x,
                        y: snapped_y,
                    });
                }
            }

            tab.is_dirty = true;

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_resize_finished(move |node_id: SharedString, width: f32, height: f32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let snapped_width = snap_to_grid(width).max(GRID_SIZE * NODE_WIDTH_CELLS);
            if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                let min_height = GRID_SIZE
                    * (NODE_MIN_ROWS
                        .max(NODE_HEADER_ROWS
                            + node.input_ports.len().max(node.output_ports.len()) as f32)
                        + NODE_PADDING_BOTTOM);
                let snapped_height = snap_to_grid(height).max(min_height);
                node.size = Some(crate::node::graph_io::GraphSize {
                    width: snapped_width,
                    height: snapped_height,
                });
            }

            tab.is_dirty = true;

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let port_selection = Arc::new(Mutex::new(None::<(String, String, bool)>));
    let port_selection_for_click = Arc::clone(&port_selection);
    let port_selection_for_move = Arc::clone(&port_selection);
    let port_selection_for_cancel = Arc::clone(&port_selection);
    let ui_handle_for_click = ui.as_weak();
    let ui_handle_for_move = ui.as_weak();
    let ui_handle_for_cancel = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);

    ui.on_port_clicked(move |node_id: SharedString, port_name: SharedString, is_input: bool| {
        let node_id_str = node_id.to_string();
        let port_name_str = port_name.to_string();

        let mut selection = port_selection_for_click.lock().unwrap();

        if let Some((prev_node, prev_port, prev_is_input)) = selection.take() {
            if prev_is_input != is_input {
                let mut tabs_guard = tabs_clone.lock().unwrap();
                let active_index = *active_tab_clone.lock().unwrap();
                if let Some(tab) = tabs_guard.get_mut(active_index) {
                    ensure_positions(&mut tab.graph);

                    let (from_node, from_port, to_node, to_port) = if is_input {
                        (prev_node, prev_port, node_id_str, port_name_str)
                    } else {
                        (node_id_str, port_name_str, prev_node, prev_port)
                    };

                    tab.graph.edges.push(crate::node::graph_io::EdgeDefinition {
                        from_node_id: from_node,
                        from_port,
                        to_node_id: to_node,
                        to_port,
                    });

                    tab.is_dirty = true;

                    if let Some(ui) = ui_handle_for_click.upgrade() {
                        ui.set_drag_line_visible(false);
                        ui.set_show_port_hint(false);
                        ui.set_port_hint_text("".into());
                        refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                    }
                }
            } else {
                *selection = Some((prev_node, prev_port, prev_is_input));
            }
        } else {
            *selection = Some((node_id_str.clone(), port_name_str.clone(), is_input));
            if let Some(ui) = ui_handle_for_click.upgrade() {
                let mut tabs_guard = tabs_clone.lock().unwrap();
                let active_index = *active_tab_clone.lock().unwrap();
                if let Some(tab) = tabs_guard.get_mut(active_index) {
                    ensure_positions(&mut tab.graph);
                    if let Some((from_x, from_y)) = get_port_center(
                        &tab.graph,
                        node_id_str.as_str(),
                        port_name_str.as_str(),
                        is_input,
                    ) {
                        ui.set_drag_line_visible(true);
                        ui.set_drag_line_from_x(from_x);
                        ui.set_drag_line_from_y(from_y);
                        ui.set_drag_line_to_x(from_x);
                        ui.set_drag_line_to_y(from_y);
                    }
                }

                if is_input {
                    ui.set_port_hint_text("连接到输出port,按右键取消".into());
                } else {
                    ui.set_port_hint_text("连接到输入port,按右键取消".into());
                }
                ui.set_show_port_hint(true);
            }
        }
    });

    ui.on_pointer_moved(move |x: f32, y: f32| {
        if port_selection_for_move.lock().unwrap().is_none() {
            return;
        }

        if let Some(ui) = ui_handle_for_move.upgrade() {
            ui.set_drag_line_to_x(snap_to_grid_center(x));
            ui.set_drag_line_to_y(snap_to_grid_center(y));
        }
    });

    ui.on_cancel_connect(move || {
        *port_selection_for_cancel.lock().unwrap() = None;
        if let Some(ui) = ui_handle_for_cancel.upgrade() {
            ui.set_drag_line_visible(false);
            ui.set_show_port_hint(false);
            ui.set_port_hint_text("".into());
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_node_clicked(move |node_id: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.select_node(node_id.to_string(), false);
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_edge_clicked(move |from_node: SharedString, from_port: SharedString, to_node: SharedString, to_port: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.select_edge(
                    from_node.to_string(),
                    from_port.to_string(),
                    to_node.to_string(),
                    to_port.to_string(),
                );
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_canvas_clicked(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.selection.clear();
                tab.selection.apply_to_ui(&ui);
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_delete_selected(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if !tab.selection.selected_node_ids.is_empty() {
                    tab.graph.nodes.retain(|n| !tab.selection.selected_node_ids.contains(&n.id));
                    tab.graph.edges.retain(|e| {
                        !tab.selection.selected_node_ids.contains(&e.from_node_id)
                            && !tab.selection.selected_node_ids.contains(&e.to_node_id)
                    });
                } else if !tab.selection.selected_edge_from_node.is_empty() {
                    tab.graph.edges.retain(|e| {
                        !(e.from_node_id == tab.selection.selected_edge_from_node
                            && e.from_port == tab.selection.selected_edge_from_port
                            && e.to_node_id == tab.selection.selected_edge_to_node
                            && e.to_port == tab.selection.selected_edge_to_port)
                    });
                }

                tab.selection.clear();
                tab.selection.apply_to_ui(&ui);
                tab.is_dirty = true;

                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_auto_layout_graph(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                crate::node::graph_io::auto_layout(&mut tab.graph);
                tab.is_dirty = true;
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let box_selection = Arc::new(Mutex::new(BoxSelection::new()));

    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    ui.on_box_selection_start(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.lock().unwrap();
        box_sel.start(x, y);

        if let Some(ui) = ui_handle.upgrade() {
            ui.set_box_selection_visible(true);
            ui.set_box_selection_x(x);
            ui.set_box_selection_y(y);
            ui.set_box_selection_width(0.0);
            ui.set_box_selection_height(0.0);
        }
    });

    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    ui.on_box_selection_update(move |x: f32, y: f32| {
        let mut box_sel = box_selection_clone.lock().unwrap();
        box_sel.update(x, y);

        if let Some(ui) = ui_handle.upgrade() {
            let (min_x, min_y, max_x, max_y) = box_sel.get_bounds();
            ui.set_box_selection_x(min_x);
            ui.set_box_selection_y(min_y);
            ui.set_box_selection_width(max_x - min_x);
            ui.set_box_selection_height(max_y - min_y);
        }
    });

    let ui_handle = ui.as_weak();
    let box_selection_clone = Arc::clone(&box_selection);
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_box_selection_end(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut box_sel = box_selection_clone.lock().unwrap();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();

            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let mut selected_nodes = Vec::new();
                for node in &tab.graph.nodes {
                    if let Some(pos) = &node.position {
                        let (node_width, node_height) = node_dimensions(node);
                        if box_sel.contains_rect(pos.x, pos.y, node_width, node_height) {
                            selected_nodes.push(node.id.clone());
                        }
                    }
                }

                tab.selection.clear();
                for node_id in selected_nodes {
                    tab.selection.select_node(node_id, true);
                }
                tab.selection.apply_to_ui(&ui);

                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
            }

            box_sel.end();
            ui.set_box_selection_visible(false);
        }
    });
}
