use std::sync::{Arc, Mutex};

use log::{error, info};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::ui::graph_window::{NodeGraphWindow, NodeTypeVm};
use crate::ui::node_graph_view::{
    refresh_active_tab_ui, tab_display_title, GraphTabState,
};
use crate::ui::node_graph_view_inline::{add_node_to_graph, apply_inline_inputs_to_graph};
use crate::ui::node_graph_view_vm::{apply_graph_to_ui, matches_node_type_search};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

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
        let (tab_id, graph_def, inline_inputs_map) = {
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

            (tab.id, tab.graph.clone(), tab.inline_inputs.clone())
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
    let all_node_types_clone = Arc::clone(&all_node_types);
    ui.on_show_node_help(move |type_id: SharedString| {
        if let Some(ui) = ui_handle.upgrade() {
            if let Some(node_type) = all_node_types_clone.iter().find(|n| n.type_id == type_id) {
                ui.set_node_help_data(node_type.clone());
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
