use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use log::{error, info};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::node::graph_io::NodeGraphDefinition;
use crate::ui::graph_window::{NodeGraphWindow, NodeTypeVm};
use crate::ui::task_manager::{push_task_list_to_ui, TASK_MANAGER};
use crate::ui::node_graph_view::{refresh_active_tab_ui, tab_display_title, GraphTabState};
use crate::ui::node_graph_view_clipboard::{
    convert_selection_to_function_subgraph, copy_selected_nodes_to_clipboard,
    paste_nodes_from_clipboard, NodeClipboard,
};
use crate::ui::node_graph_view_geometry::{node_dimensions, snap_to_grid};
use crate::ui::node_graph_view_inline::{add_node_to_graph, materialize_graph_for_execution};
use crate::ui::node_graph_view_vm::{apply_graph_to_ui, matches_node_type_search};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

fn filter_node_types_for_page(all_node_types: &[NodeTypeVm], is_subgraph_page: bool) -> Vec<NodeTypeVm> {
    all_node_types
        .iter()
        .filter(|node_type| {
            let type_id = node_type.type_id.as_str();
            if matches!(type_id, "function_inputs" | "function_outputs") {
                return false;
            }
            if is_subgraph_page && crate::node::registry::NODE_REGISTRY.is_event_producer(type_id) {
                return false;
            }
            true
        })
        .cloned()
        .collect()
}

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
                        .map(|source_node| {
                            format!("已连接自：{} · {}", source_node.name, edge.from_port)
                        })
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
                        .map(|target_node| {
                            format!("已连接到：{} · {}", target_node.name, edge.to_port)
                        })
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

fn clear_graph_error_state(graph: &mut NodeGraphDefinition) {
    for node in &mut graph.nodes {
        node.has_error = false;
        node.has_cycle = false;
    }
}

fn graph_has_error_state(graph: &NodeGraphDefinition) -> bool {
    graph
        .nodes
        .iter()
        .any(|node| node.has_error || node.has_cycle)
}

fn collect_error_related_node_ids(
    _graph: &NodeGraphDefinition,
    error_node_id: &str,
) -> HashSet<String> {
    let mut related_node_ids = HashSet::new();
    related_node_ids.insert(error_node_id.to_string());
    related_node_ids
}

fn mark_graph_error_path(graph: &mut NodeGraphDefinition, error_node_id: &str) {
    clear_graph_error_state(graph);

    for related_node_id in collect_error_related_node_ids(graph, error_node_id) {
        if let Some(node) = graph
            .nodes
            .iter_mut()
            .find(|node| node.id == related_node_id)
        {
            node.has_error = true;
        }
    }
}

fn is_cycle_dependency_error(error_msg: &str) -> bool {
    error_msg.contains("Cycle detected in node dependencies")
}

fn mark_graph_cycle_path(graph: &mut NodeGraphDefinition) {
    clear_graph_error_state(graph);

    for cycle_node_id in crate::node::graph_io::find_cycle_node_ids(graph) {
        if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == cycle_node_id) {
            node.has_cycle = true;
        }
    }
}

fn format_cycle_error_message(error_msg: &str) -> String {
    let display_msg = if let Some(idx) = error_msg.find("Validation error: ") {
        &error_msg[idx + "Validation error: ".len()..]
    } else {
        error_msg
    };
    format!("节点图存在环路依赖，已标黄相关节点: {}", display_msg)
}

fn format_execution_error_message(
    graph: &NodeGraphDefinition,
    error_node_id: &str,
    error_msg: &str,
) -> String {
    if is_cycle_dependency_error(error_msg) {
        return format_cycle_error_message(error_msg);
    }
    let display_msg = if let Some(idx) = error_msg.find(" [NODE_ERROR:") {
        &error_msg[..idx]
    } else {
        error_msg
    };
    if let Some(node) = graph.nodes.iter().find(|node| node.id == error_node_id) {
        format!("节点 \"{}\" 执行失败: {}", node.name, display_msg)
    } else {
        format!("节点 {} 执行失败: {}", error_node_id, display_msg)
    }
}

fn extract_error_node_id(error_msg: &str) -> Option<String> {
    if let Some(start) = error_msg.find("[NODE_ERROR:") {
        if let Some(end) = error_msg[start + 12..].find(']') {
            return Some(error_msg[start + 12..start + 12 + end].to_string());
        }
    }

    if let Some(start) = error_msg.find("Node '") {
        if let Some(end) = error_msg[start + 6..].find('\'') {
            return Some(error_msg[start + 6..start + 6 + end].to_string());
        }
    }

    None
}

fn current_canvas_viewport_center(ui: &NodeGraphWindow) -> (f32, f32) {
    let zoom = ui.get_canvas_zoom().max(0.2);
    let pan_x = ui.get_canvas_pan_x();
    let pan_y = ui.get_canvas_pan_y();
    let viewport_width = ui.get_canvas_viewport_width().max(1.0);
    let viewport_height = ui.get_canvas_viewport_height().max(1.0);

    (
        viewport_width / zoom / 2.0 - pan_x,
        viewport_height / zoom / 2.0 - pan_y,
    )
}

pub(crate) fn bind_window_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
    all_node_types: Arc<Vec<NodeTypeVm>>,
    node_clipboard: Arc<Mutex<Option<NodeClipboard>>>,
    last_context_canvas_pos: Arc<Mutex<Option<(f32, f32)>>>,
    pending_add_node_pos: Arc<Mutex<Option<(f32, f32)>>>,
) {
    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let pending_add_node_pos_clone = Arc::clone(&pending_add_node_pos);
    ui.on_add_node(move |type_id: SharedString| {
        let type_id_str = type_id.as_str();
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            if tab.is_subgraph_page()
                && crate::node::registry::NODE_REGISTRY.is_event_producer(type_id_str)
            {
                if let Some(ui) = ui_handle.upgrade() {
                    ui.invoke_show_error("函数子图内不能添加事件源节点".into());
                }
                return;
            }
            if let Err(e) = add_node_to_graph(&mut tab.graph, type_id_str) {
                eprintln!("Failed to add node: {}", e);
                return;
            }

            if let Some(node) = tab.graph.nodes.last_mut() {
                let (node_width, node_height) = node_dimensions(node);
                let context_pos = pending_add_node_pos_clone.lock().unwrap().take();

                let (center_canvas_x, center_canvas_y) = if let Some((x, y)) = context_pos {
                    (x, y)
                } else {
                    ui_handle
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
                                viewport_width / zoom / 2.0 - pan_x,
                                viewport_height / zoom / 2.0 - pan_y,
                            )
                        })
                        .unwrap_or((0.0, 0.0))
                };

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
        let (tab_id, tab_title, graph_def, inline_inputs_map, hyperparameter_values) = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let tab = match tabs_guard.get(active_index) {
                Some(tab) => tab,
                None => return,
            };

            if tab.is_subgraph_page() {
                if let Some(ui) = ui_handle.upgrade() {
                    ui.invoke_show_error("子图页面不能直接运行，请返回主图运行".into());
                }
                return;
            }

            if tab.is_running {
                info!("节点图已在运行中");
                return;
            }

            (
                tab.id,
                tab_display_title(tab),
                tab.graph.clone(),
                tab.inline_inputs.clone(),
                tab.hyperparameter_values.clone(),
            )
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
                    ui.set_error_dialog_message(
                        format!("必填超参数 '{}' 未设置值，无法运行节点图", hp.name).into(),
                    );
                }
                return;
            }
        }

        materialize_graph_for_execution(&mut graph_def, &inline_inputs_map, &hyperparameter_values);
        clear_graph_error_state(&mut graph_def);

        match crate::node::registry::build_node_graph_from_definition(&graph_def) {
            Ok(mut node_graph) => {
                info!("开始执行节点图...");

                let stop_flag_clone = node_graph.get_stop_flag();

                // Register task in global task manager
                let task_id = TASK_MANAGER.lock().unwrap().add_task(&tab_title, Some(stop_flag_clone.clone()));
                if let Some(ui) = ui_handle.upgrade() {
                    push_task_list_to_ui(&ui);
                }

                let has_event_producer = node_graph
                    .nodes
                    .values()
                    .any(|node| node.node_type() == crate::node::NodeType::EventProducer);

                if has_event_producer {
                    let stop_flag = stop_flag_clone;

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
                                if graph_has_error_state(&tab.graph) {
                                    clear_graph_error_state(&mut tab.graph);
                                }
                                tab.graph.execution_results.insert(node_id, result);
                                if let Some(ui) = ui_weak_cb.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            tab.root_graph().variables.as_slice(),
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

                            if let Some(error_msg) = execution_result.error_message.clone() {
                                error!("节点图执行失败: {}", error_msg);
                                let display_error = if is_cycle_dependency_error(&error_msg) {
                                    mark_graph_cycle_path(&mut tab.graph);
                                    format_cycle_error_message(&error_msg)
                                } else {
                                    let error_node_id = execution_result
                                        .error_node_id
                                        .clone()
                                        .unwrap_or_else(|| "unknown".to_string());
                                    mark_graph_error_path(&mut tab.graph, &error_node_id);
                                    format_execution_error_message(
                                        &tab.graph,
                                        &error_node_id,
                                        &error_msg,
                                    )
                                };

                                if let Some(ui) = ui_weak.upgrade() {
                                    if active_tab_id == Some(tab_id) {
                                        apply_graph_to_ui(
                                            &ui,
                                            &tab.graph,
                                            tab.root_graph().variables.as_slice(),
                                            Some(tab_display_title(tab)),
                                            &tab.selection,
                                            &inline_inputs_bg,
                                            &hp_values_bg,
                                        );
                                        ui.invoke_show_error(display_error.clone().into());
                                        ui.set_connection_status(
                                            format!("❌ {}", display_error).into(),
                                        );
                                    }
                                }
                            } else {
                                if stop_flag.load(std::sync::atomic::Ordering::Relaxed) {
                                    info!("节点图执行已停止");
                                } else {
                                    info!("节点图执行成功!");
                                }

                                clear_graph_error_state(&mut tab.graph);

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
                                            tab.root_graph().variables.as_slice(),
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

                            TASK_MANAGER.lock().unwrap().finish_task(task_id);
                            if let Some(ui) = ui_weak.upgrade() {
                                push_task_list_to_ui(&ui);
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

                    if let Some(error_msg) = execution_result.error_message.clone() {
                        error!("节点图执行失败: {}", error_msg);
                        let display_error = if is_cycle_dependency_error(&error_msg) {
                            mark_graph_cycle_path(&mut tab.graph);
                            format_cycle_error_message(&error_msg)
                        } else {
                            let error_node_id = execution_result
                                .error_node_id
                                .clone()
                                .unwrap_or_else(|| "unknown".to_string());
                            mark_graph_error_path(&mut tab.graph, &error_node_id);
                            format_execution_error_message(&tab.graph, &error_node_id, &error_msg)
                        };

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    tab.root_graph().variables.as_slice(),
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                    &hyperparameter_values,
                                );
                                ui.invoke_show_error(display_error.clone().into());
                                ui.set_connection_status(format!("❌ {}", display_error).into());
                            }
                        }
                    } else {
                        info!("节点图执行成功!");
                        clear_graph_error_state(&mut tab.graph);

                        if let Some(ui) = ui_handle.upgrade() {
                            if active_tab_id == Some(tab_id) {
                                ui.set_connection_status("节点图执行成功".into());
                                apply_graph_to_ui(
                                    &ui,
                                    &tab.graph,
                                    tab.root_graph().variables.as_slice(),
                                    Some(tab_display_title(tab)),
                                    &tab.selection,
                                    &inline_inputs_map,
                                    &hyperparameter_values,
                                );
                            }
                        }
                    }
                    // Mark task as finished (non-EventProducer synchronous path)
                    TASK_MANAGER.lock().unwrap().finish_task(task_id);
                    if let Some(ui) = ui_handle.upgrade() {
                        push_task_list_to_ui(&ui);
                    }
                }
            }
            Err(e) => {
                error!("构建节点图失败: {}", e);
                if let Some(ui) = ui_handle.upgrade() {
                    let error_msg = e.to_string();
                    ui.invoke_show_error(
                        extract_error_node_id(&error_msg)
                            .map(|error_node_id| {
                                format_execution_error_message(
                                    &graph_def,
                                    &error_node_id,
                                    &error_msg,
                                )
                            })
                            .unwrap_or_else(|| format!("构建节点图失败：{}", error_msg))
                            .into(),
                    );
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
    ui.on_stop_task(move |task_id_str| {
        if let Ok(task_id) = task_id_str.trim().parse::<u64>() {
            info!("请求停止任务: {}", task_id);
            let manager = TASK_MANAGER.lock().unwrap();
            manager.stop_task(task_id);
            drop(manager);

            if let Some(ui) = ui_handle.upgrade() {
                push_task_list_to_ui(&ui);
            }
        }
    });

    let ui_handle = ui.as_weak();
    ui.on_hide_graph_context_menu(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_graph_context_menu(false);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let node_clipboard_clone = Arc::clone(&node_clipboard);
    ui.on_copy_selected_nodes(move || {
        let new_clipboard = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            tabs_guard.get(active_index).and_then(|tab| {
                copy_selected_nodes_to_clipboard(
                    &tab.graph,
                    &tab.inline_inputs,
                    &tab.selection.selected_node_ids,
                )
            })
        };

        let can_paste = if let Some(clipboard) = new_clipboard {
            *node_clipboard_clone.lock().unwrap() = Some(clipboard);
            true
        } else {
            node_clipboard_clone.lock().unwrap().is_some()
        };

        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_graph_context_menu(false);
            ui.set_graph_context_menu_can_paste(can_paste);
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    ui.on_convert_selection_to_function_subgraph(move || {
        let Some(ui) = ui_handle.upgrade() else {
            return;
        };

        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        let Some(tab) = tabs_guard.get_mut(active_index) else {
            ui.set_show_graph_context_menu(false);
            return;
        };

        match convert_selection_to_function_subgraph(
            &tab.graph,
            &tab.inline_inputs,
            &tab.selection.selected_node_ids,
        ) {
            Ok(result) => {
                tab.graph = result.graph;
                tab.inline_inputs = result.inline_inputs;
                tab.selection.clear();
                tab.selection
                    .select_node(result.function_node_id.clone(), false);
                tab.is_dirty = true;
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
            Err(message) => {
                ui.set_show_graph_context_menu(false);
                ui.invoke_show_error(message.into());
            }
        }
    });

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let node_clipboard_clone = Arc::clone(&node_clipboard);
    let last_context_canvas_pos_clone = Arc::clone(&last_context_canvas_pos);
    ui.on_paste_nodes_at_context(move || {
        let clipboard = node_clipboard_clone.lock().unwrap().clone();
        let context_pos = last_context_canvas_pos_clone
            .lock()
            .unwrap()
            .as_ref()
            .copied()
            .or_else(|| {
                ui_handle
                    .upgrade()
                    .map(|ui| current_canvas_viewport_center(&ui))
            });

        if let (Some(clipboard), Some((x, y))) = (clipboard, context_pos) {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(pasted) = paste_nodes_from_clipboard(&tab.graph, &clipboard, x, y) {
                    let pasted_node_ids = pasted.pasted_node_ids.clone();
                    tab.graph.nodes.extend(pasted.nodes);
                    tab.graph.edges.extend(pasted.edges);
                    tab.inline_inputs.extend(pasted.inline_inputs);
                    tab.selection.clear();
                    for node_id in pasted_node_ids {
                        tab.selection.select_node(node_id, true);
                    }
                    tab.is_dirty = true;
                }

                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_show_graph_context_menu(false);
                    refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                }
            }
        } else if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_graph_context_menu(false);
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    let last_context_canvas_pos_clone = Arc::clone(&last_context_canvas_pos);
    let pending_add_node_pos_clone = Arc::clone(&pending_add_node_pos);
    let tabs_for_context_new = Arc::clone(&tabs);
    let active_tab_for_context_new = Arc::clone(&active_tab_index);
    ui.on_context_menu_new_node(move || {
        *pending_add_node_pos_clone.lock().unwrap() =
            *last_context_canvas_pos_clone.lock().unwrap();

        if let Some(ui) = ui_handle.upgrade() {
            let is_subgraph_page = {
                let tabs_guard = tabs_for_context_new.lock().unwrap();
                let active_index = *active_tab_for_context_new.lock().unwrap();
                tabs_guard
                    .get(active_index)
                    .map(|tab| tab.is_subgraph_page())
                    .unwrap_or(false)
            };
            ui.set_available_node_types(ModelRc::new(VecModel::from(
                filter_node_types_for_page(all_node_types_clone.as_ref(), is_subgraph_page),
            )));
            ui.set_show_graph_context_menu(false);
            ui.set_show_node_selector(true);
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    let tabs_for_filter = Arc::clone(&tabs);
    let active_tab_for_filter = Arc::clone(&active_tab_index);
    ui.on_filter_nodes(move |search_text: SharedString, category: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            let search_text = search_text.as_str().to_lowercase();
            let category = category.as_str();
            let is_subgraph_page = {
                let tabs_guard = tabs_for_filter.lock().unwrap();
                let active_index = *active_tab_for_filter.lock().unwrap();
                tabs_guard
                    .get(active_index)
                    .map(|tab| tab.is_subgraph_page())
                    .unwrap_or(false)
            };

            let filtered: Vec<NodeTypeVm> = filter_node_types_for_page(
                all_node_types_clone.as_ref(),
                is_subgraph_page,
            )
            .into_iter()
            .filter(|n| {
                let name_match = matches_node_type_search(n, &search_text);
                let cat_match = category.is_empty() || n.category == category;
                name_match && cat_match
            })
            .collect();

            ui.set_available_node_types(ModelRc::new(VecModel::from(filtered)));
        }
    });

    let ui_handle = ui.as_weak();
    let all_node_types_clone = Arc::clone(&all_node_types);
    let pending_add_node_pos_clone = Arc::clone(&pending_add_node_pos);
    let tabs_for_show = Arc::clone(&tabs);
    let active_tab_for_show = Arc::clone(&active_tab_index);
    ui.on_show_node_type_menu(move || {
        pending_add_node_pos_clone.lock().unwrap().take();
        if let Some(ui) = ui_handle.upgrade() {
            let is_subgraph_page = {
                let tabs_guard = tabs_for_show.lock().unwrap();
                let active_index = *active_tab_for_show.lock().unwrap();
                tabs_guard
                    .get(active_index)
                    .map(|tab| tab.is_subgraph_page())
                    .unwrap_or(false)
            };
            ui.set_available_node_types(ModelRc::new(VecModel::from(
                filter_node_types_for_page(all_node_types_clone.as_ref(), is_subgraph_page),
            )));
            ui.set_show_graph_context_menu(false);
            ui.set_show_node_selector(true);
        }
    });

    let ui_handle = ui.as_weak();
    let pending_add_node_pos_clone = Arc::clone(&pending_add_node_pos);
    ui.on_hide_node_type_menu(move || {
        pending_add_node_pos_clone.lock().unwrap().take();
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

    // Task manager toggle
    let ui_handle = ui.as_weak();
    ui.on_toggle_task_manager(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let show = !ui.get_show_task_manager();
            ui.set_show_task_manager(show);
        }
    });
}
