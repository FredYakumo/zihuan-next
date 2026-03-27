use std::sync::{Arc, Mutex};

use log::{error, info};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::node::graph_io::NodeGraphDefinition;
use crate::ui::graph_window::{NodeGraphWindow, NodeTypeVm};
use crate::ui::node_graph_view::{
    refresh_active_tab_ui, tab_display_title, GraphTabState,
};
use crate::ui::node_graph_view_geometry::{node_dimensions, snap_to_grid};
use crate::ui::node_graph_view_inline::{add_node_to_graph, apply_hyperparameter_bindings_to_graph, apply_inline_inputs_to_graph};
use crate::ui::node_graph_view_vm::{apply_graph_to_ui, matches_node_type_search};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

fn build_node_help_data(
    node_type: &NodeTypeVm,
    graph: &NodeGraphDefinition,
    node_id: &str,
) -> NodeTypeVm {
    let node = graph.nodes.iter().find(|candidate| candidate.id == node_id);

    let input_ports = node_type
        .input_ports
        .iter()
        .map(|port| {
            let connection_text = graph
                .edges
                .iter()
                .find(|edge| edge.to_node_id == node_id && edge.to_port == port.name.as_str())
                .and_then(|edge| {
                    graph
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == edge.from_node_id)
                        .map(|source_node| format!("已连接自：{} · {}", source_node.name, edge.from_port))
                })
                .unwrap_or_default();

            crate::ui::graph_window::PortHelpVm {
                name: port.name.clone(),
                data_type: port.data_type.clone(),
                description: port.description.clone(),
                required: port.required,
                connection_text: connection_text.into(),
            }
        })
        .collect::<Vec<_>>();

    let output_ports = node_type
        .output_ports
        .iter()
        .map(|port| {
            let mut connection_lines = graph
                .edges
                .iter()
                .filter(|edge| edge.from_node_id == node_id && edge.from_port == port.name.as_str())
                .filter_map(|edge| {
                    graph
                        .nodes
                        .iter()
                        .find(|candidate| candidate.id == edge.to_node_id)
                        .map(|target_node| format!("已连接到：{} · {}", target_node.name, edge.to_port))
                })
                .collect::<Vec<_>>();

            let overflow = connection_lines.len().saturating_sub(2);
            if connection_lines.len() > 2 {
                connection_lines.truncate(2);
                connection_lines.push(format!("等...（其余 {} 个）", overflow));
            }

            crate::ui::graph_window::PortHelpVm {
                name: port.name.clone(),
                data_type: port.data_type.clone(),
                description: port.description.clone(),
                required: port.required,
                connection_text: connection_lines.join("\n").into(),
            }
        })
        .collect::<Vec<_>>();

    NodeTypeVm {
        type_id: node_type.type_id.clone(),
        display_name: node
            .map(|node| node.name.as_str())
            .unwrap_or(node_type.display_name.as_str())
            .into(),
        category: node_type.category.clone(),
        description: node
            .and_then(|node| node.description.as_deref())
            .unwrap_or(node_type.description.as_str())
            .into(),
        input_ports: ModelRc::new(VecModel::from(input_ports)),
        output_ports: ModelRc::new(VecModel::from(output_ports)),
    }
}

pub(crate) fn bind_window_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
    all_node_types: Arc<Vec<NodeTypeVm>>,
) {
    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_add_node(move |type_id: SharedString| {
        let type_id_str = type_id.as_str();
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Err(e) = add_node_to_graph(&mut tab.graph, type_id_str) {
                eprintln!("Failed to add node: {}", e);
                return;
            }

            if let Some(node) = tab.graph.nodes.last_mut() {
                let (pan_x, pan_y, viewport_width, viewport_height) = ui_handle
                    .upgrade()
                    .map(|ui| {
                        (
                            ui.get_canvas_pan_x(),
                            ui.get_canvas_pan_y(),
                            ui.get_canvas_zoom().max(0.2),
                            ui.get_canvas_viewport_width().max(1.0),
                            ui.get_canvas_viewport_height().max(1.0),
                        )
                    })
                    .map(|(pan_x, pan_y, zoom, viewport_width, viewport_height)| {
                        (
                            pan_x,
                            pan_y,
                            viewport_width / zoom,
                            viewport_height / zoom,
                        )
                    })
                    .unwrap_or((0.0, 0.0, 1200.0, 800.0));

                let center_canvas_x = viewport_width / 2.0 - pan_x;
                let center_canvas_y = viewport_height / 2.0 - pan_y;
                let (node_width, node_height) = node_dimensions(node);

                node.position = Some(crate::node::graph_io::GraphPosition {
                    x: snap_to_grid(center_canvas_x - node_width / 2.0),
                    y: snap_to_grid(center_canvas_y - node_height / 2.0),
                });
            }
            tab.is_dirty = true;
        }

        if let Some(ui) = ui_handle.upgrade() {
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_run_graph(move || {
        let (tab_id, graph_def, inline_inputs_map, hyperparameter_values) = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let tab = match tabs_guard.get(active_index) {
                Some(tab) => tab,
                None => return,
            };

            if tab.is_running {
                info!("节点图已在运行中");
                return;
            }

            (tab.id, tab.graph.clone(), tab.inline_inputs.clone(), tab.hyperparameter_values.clone())
        };

        let mut graph_def = graph_def;

        {
            use crate::node::util::STRING_DATA_CONTEXT;
            let mut context = STRING_DATA_CONTEXT.write().unwrap();
            context.clear();

            for node in &graph_def.nodes {
                if node.node_type == "string_data" {
                    let key = inline_port_key(&node.id, "text");
                    if let Some(InlinePortValue::Text(value)) = inline_inputs_map.get(&key) {
                        context.insert(node.id.clone(), value.clone());
                    }
                }
            }
        }

        // Validate required hyperparameters
        for hp in &graph_def.hyperparameters {
            if hp.required && !hyperparameter_values.contains_key(&hp.name) {
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_show_error_dialog(true);
                    ui.set_error_dialog_message(format!("必填超参数 '{}' 未设置值，无法运行节点图", hp.name).into());
                }
                return;
            }
        }

        apply_hyperparameter_bindings_to_graph(&mut graph_def, &hyperparameter_values);
        apply_inline_inputs_to_graph(&mut graph_def, &inline_inputs_map);

        match crate::node::registry::build_node_graph_from_definition(&graph_def) {
            Ok(mut node_graph) => {
                info!("开始执行节点图...");

                let has_event_producer = node_graph
                    .nodes
                    .values()
                    .any(|node| node.node_type() == crate::node::NodeType::EventProducer);

                if has_event_producer {
                    let stop_flag = node_graph.get_stop_flag();

                    {
                        let mut tabs_guard = tabs_clone.lock().unwrap();
                        if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                            tab.is_running = true;
                            tab.stop_flag = Some(stop_flag.clone());
                        }
                    }

                    if let Some(ui) = ui_handle.upgrade() {
                        let active_index = *active_tab_clone.lock().unwrap();
                        let tabs_guard = tabs_clone.lock().unwrap();
                        if let Some(tab) = tabs_guard.get(active_index) {
                            if tab.id == tab_id {
                                ui.set_is_graph_running(true);
                                ui.set_connection_status("⏳ 节点图运行中...".into());
                            }
                        }
                    }

                    let tabs_cb = Arc::clone(&tabs_clone);
                    let ui_weak_cb = ui_handle.clone();
                    let active_tab_cb = Arc::clone(&active_tab_clone);
                    let inline_inputs_cb = inline_inputs_map.clone();
                    let hp_values_cb = hyperparameter_values.clone();

                    node_graph.set_execution_callback(move |node_id, inputs, outputs| {
                        let node_id = node_id.to_string();
                        let mut result = inputs.clone();
                        for (k, v) in outputs {
                            result.insert(k.clone(), v.clone());
                        }

                        let tabs_cb = Arc::clone(&tabs_cb);
                        let ui_weak_cb = ui_weak_cb.clone();
                        let active_tab_cb = Arc::clone(&active_tab_cb);
                        let inline_inputs_cb = inline_inputs_cb.clone();
                        let hp_values_cb = hp_values_cb.clone();

                        let _ = slint::invoke_from_event_loop(move || {
                            let mut tabs_guard = tabs_cb.lock().unwrap();
                            let active_index = *active_tab_cb.lock().unwrap();
                            let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                            if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                                tab.graph.execution_results.insert(node_id, result);
                                if let Some(ui) = ui_weak_cb.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_cb,
                                            &hp_values_cb,
                                        );
                                    }
                                }
                            }
                        });
                    });

                    let ui_weak = ui_handle.clone();
                    let tabs_bg = Arc::clone(&tabs_clone);
                    let active_tab_bg = Arc::clone(&active_tab_clone);
                    let inline_inputs_bg = inline_inputs_map.clone();
                    let hp_values_bg = hyperparameter_values.clone();

                    std::thread::spawn(move || {
                        let execution_result = node_graph.execute_and_capture_results();

                        let _ = slint::invoke_from_event_loop(move || {
                            let mut tabs_guard = tabs_bg.lock().unwrap();
                            let active_index = *active_tab_bg.lock().unwrap();
                            let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                            let tab = match tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                                Some(tab) => tab,
                                None => return,
                            };

                            tab.graph.execution_results = execution_result.node_results;

                            if let (Some(error_node_id), Some(error_msg)) = (
                                execution_result.error_node_id.clone(),
                                execution_result.error_message.clone(),
                            ) {
                                error!("节点图执行失败: {}", error_msg);
                                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == error_node_id) {
                                    node.has_error = true;
                                }

                                if let Some(ui) = ui_weak.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_bg,
                                            &hp_values_bg,
                                        );
                                        ui.invoke_show_error(format!("执行错误：{}", error_msg).into());
                                        ui.set_connection_status(format!("❌ 执行失败: {}", error_msg).into());
                                    }
                                }
                            } else {
                                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                    info!("节点图执行已停止");
                                } else {
                                    info!("节点图执行成功!");
                                }

                                for node in &mut tab.graph.nodes {
                                    node.has_error = false;
                                }

                                if let Some(ui) = ui_weak.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                            ui.set_connection_status("节点图执行已停止".into());
                                        } else {
                                            ui.set_connection_status("节点图执行成功".into());
                                        }
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_bg,
                                            &hp_values_bg,
                                        );
                                    }
                                }
                            }

                            tab.is_running = false;
                            tab.stop_flag = None;

                            if let Some(ui) = ui_weak.upgrade() {
                                if active_tab_id == Some(tab_id) {
                                    ui.set_is_graph_running(false);
                                }
                            }
                        });
                    });
                } else {
                    let execution_result = node_graph.execute_and_capture_results();

                    let mut tabs_guard = tabs_clone.lock().unwrap();
                    let active_index = *active_tab_clone.lock().unwrap();
                    let active_tab_id = tabs_guard.get(active_index).map(|t| t.id);
                    let tab = match tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                        Some(tab) => tab,
                        None => return,
                    };

                    tab.graph.execution_results = execution_result.node_results;

                    if let (Some(error_node_id), Some(error_msg)) = (
                        execution_result.error_node_id.clone(),
                        execution_result.error_message.clone(),
                    ) {
                        error!("节点图执行失败: {}", error_msg);
                        if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == error_node_id) {
                            node.has_error = true;
                        }

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                    &hyperparameter_values,
                                );
                                ui.invoke_show_error(format!("执行错误：{}", error_msg).into());
                            }
                        }
                    } else {
                        info!("节点图执行成功!");
                        for node in &mut tab.graph.nodes {
                            node.has_error = false;
                        }

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                ui.set_connection_status("节点图执行成功".into());
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                    &hyperparameter_values,
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("构建节点图失败: {}", e);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.invoke_show_error(format!("构建节点图失败：{}", e).into());
                }
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_stop_graph(move || {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if let Some(stop_flag) = tab.stop_flag.as_ref() {
                info!("请求停止节点图执行");
                stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_filter_nodes(move |search_text: SharedString, category: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let search_text = search_text.as_str().to_lowercase();
            let category = category.as_str();

            let filtered: Vec<NodeTypeVm> = all_node_types_clone
                .iter()
                .filter(|n| {
                    let name_match = matches_node_type_search(n, &search_text);
                    let cat_match = category.is_empty() || n.category == category;
                    name_match && cat_match
                })
                .cloned()
                .collect();

            ui.set_available_node_types(ModelRc::new(VecModel::from(filtered)));
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_show_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_available_node_types(ModelRc::new(VecModel::from(all_node_types_clone.as_ref().clone())));
            ui.set_show_node_selector(true);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_node_type_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_node_selector(false);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_show_error(move |message: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_error_dialog_message(message);
            ui.set_show_error_dialog(true);
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_error(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_error_dialog(false);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_show_node_help(move |node_id: SharedString, type_id: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            if let Some(node_type) = all_node_types_clone.iter().find(|n| n.type_id == type_id) {
                let active_index = *active_tab_clone.lock().unwrap();
                let tabs_guard = tabs_clone.lock().unwrap();
                let node_help_data = tabs_guard
                    .get(active_index)
                    .map(|tab| build_node_help_data(node_type, &tab.graph, node_id.as_str()))
                    .unwrap_or_else(|| node_type.clone());

                ui.set_node_help_data(node_help_data);
                ui.set_show_node_help_dialog(true);
            }
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_node_help(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_node_help_dialog(false);
        }
    });
}
