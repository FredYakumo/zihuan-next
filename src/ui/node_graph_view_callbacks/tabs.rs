use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use log::info;
use slint::ComponentHandle;

use crate::node::graph_io::{
    auto_fix_graph_definition, load_graph_definition_from_json, validate_graph_definition,
    NodeGraphDefinition,
};
use crate::ui::canvas_state::load_canvas_view_state;
use crate::ui::graph_window::{NodeGraphWindow, ValidationIssueVm};
#[cfg(target_os = "macos")]
use crate::ui::macos_menu::{install_menu, MenuActions};
use crate::ui::node_graph_view::{
    apply_canvas_view_state, new_blank_tab, persist_tab_canvas_state, refresh_active_tab_ui,
    sync_active_tab_canvas_state, GraphTabState,
};
use crate::ui::node_graph_view_inline::{
    apply_inline_inputs_to_graph, build_inline_inputs_from_graph,
};
use crate::util::hyperparam_store::{load_hyperparameter_values, save_hyperparameter_values};

pub(crate) fn bind_tab_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
    next_untitled_index: Arc<Mutex<usize>>,
    next_tab_id: Arc<Mutex<u64>>,
    pending_close_tab_id: Arc<Mutex<Option<u64>>>,
    pending_open_graph: Arc<Mutex<Option<(PathBuf, NodeGraphDefinition)>>>,
) {
    let pending_save_as: Arc<Mutex<Option<(std::path::PathBuf, u64)>>> = Arc::new(Mutex::new(None));

    let ui_handle = ui.as_weak();
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let pending_open_for_open = Arc::clone(&pending_open_graph);
    ui.on_open_json(move || {
        let selected_path = match rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .pick_file()
        {
            Some(path) => path,
            None => return,
        };

        match load_graph_definition_from_json(&selected_path) {
            Ok(graph) => {
                let issues = validate_graph_definition(&graph);
                if !issues.is_empty() {
                    // Store the pending graph and show the validation dialog
                    *pending_open_for_open.lock().unwrap() = Some((selected_path, graph));
                    let issue_vms: Vec<ValidationIssueVm> = issues
                        .iter()
                        .map(|i| ValidationIssueVm {
                            severity: i.severity.clone().into(),
                            message: i.message.clone().into(),
                        })
                        .collect();
                    if let Some(ui) = ui_handle.upgrade() {
                        ui.set_validation_issues(slint::ModelRc::from(std::rc::Rc::new(
                            slint::VecModel::from(issue_vms),
                        )));
                        ui.set_show_validation_fix_dialog(true);
                    }
                    return;
                }
                // No issues — load immediately
                if let Some(ui) = ui_handle.upgrade() {
                    let mut tabs_guard = tabs_clone.lock().unwrap();
                    let active_index = *active_tab_clone.lock().unwrap();
                    sync_active_tab_canvas_state(&ui, &mut tabs_guard, active_index);
                    if let Some(current_tab) = tabs_guard.get(active_index) {
                        persist_tab_canvas_state(current_tab);
                    }
                    if let Some(tab) = tabs_guard.get_mut(active_index) {
                        tab.graph = graph.clone();
                        tab.inline_inputs = build_inline_inputs_from_graph(&graph);
                        tab.hyperparameter_values =
                            load_hyperparameter_values(&selected_path, &tab.graph);
                        tab.canvas_view_state =
                            load_canvas_view_state(&selected_path).unwrap_or_default();
                        tab.selection.clear();
                        tab.file_path = Some(selected_path.clone());
                        tab.title = selected_path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_else(|| selected_path.display().to_string());
                        tab.is_dirty = false;
                    }

                    refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                    if let Some(tab) = tabs_guard.get(active_index) {
                        apply_canvas_view_state(&ui, &tab.canvas_view_state);
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to load graph: {}", e);
                if let Some(ui) = ui_handle.upgrade() {
                    ui.set_show_error_dialog(true);
                    ui.set_error_dialog_message(format!("无法加载文件:\n{}", e).into());
                }
            }
        }
    });

    // ── Validation Fix: confirm (auto-fix + load) ──
    let pending_open_for_confirm = Arc::clone(&pending_open_graph);
    let tabs_for_confirm = Arc::clone(&tabs);
    let active_tab_for_confirm = Arc::clone(&active_tab_index);
    let ui_handle_confirm = ui.as_weak();
    ui.on_validation_fix_confirm(move || {
        if let Some(ui) = ui_handle_confirm.upgrade() {
            ui.set_show_validation_fix_dialog(false);
        }
        let (selected_path, mut graph) = match pending_open_for_confirm.lock().unwrap().take() {
            Some(v) => v,
            None => return,
        };
        auto_fix_graph_definition(&mut graph);
        if let Some(ui) = ui_handle_confirm.upgrade() {
            let mut tabs_guard = tabs_for_confirm.lock().unwrap();
            let active_index = *active_tab_for_confirm.lock().unwrap();
            sync_active_tab_canvas_state(&ui, &mut tabs_guard, active_index);
            if let Some(current_tab) = tabs_guard.get(active_index) {
                persist_tab_canvas_state(current_tab);
            }
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.inline_inputs = build_inline_inputs_from_graph(&graph);
                tab.hyperparameter_values = load_hyperparameter_values(&selected_path, &graph);
                tab.canvas_view_state = load_canvas_view_state(&selected_path).unwrap_or_default();
                tab.selection.clear();
                tab.file_path = Some(selected_path.clone());
                tab.title = selected_path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| selected_path.display().to_string());
                tab.is_dirty = true; // mark dirty since we fixed but haven't saved
                tab.graph = graph;
            }
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            if let Some(tab) = tabs_guard.get(active_index) {
                apply_canvas_view_state(&ui, &tab.canvas_view_state);
            }
        }
    });

    // ── Validation Fix: cancel ──
    let pending_open_for_cancel = Arc::clone(&pending_open_graph);
    let ui_handle_cancel = ui.as_weak();
    ui.on_validation_fix_cancel(move || {
        pending_open_for_cancel.lock().unwrap().take();
        if let Some(ui) = ui_handle_cancel.upgrade() {
            ui.set_show_validation_fix_dialog(false);
        }
    });

    let save_tab = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| -> bool {
            let path: Option<PathBuf> = {
                let tabs_guard = tabs_clone.lock().unwrap();
                let tab_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                    Some(index) => index,
                    None => return false,
                };

                tabs_guard[tab_index].file_path.clone()
            };

            let path = if let Some(path) = path {
                path
            } else {
                match rfd::FileDialog::new()
                    .add_filter("Node Graph", &["json"])
                    .set_file_name("node_graph.json")
                    .save_file()
                {
                    Some(path) => path,
                    None => return false,
                }
            };

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let tab_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                Some(index) => index,
                None => return false,
            };

            if let Some(ui) = ui_handle.upgrade() {
                let active_index = *active_tab_clone.lock().unwrap();
                if active_index == tab_index {
                    sync_active_tab_canvas_state(&ui, &mut tabs_guard, active_index);
                }
            }

            let tab = &mut tabs_guard[tab_index];
            apply_inline_inputs_to_graph(&mut tab.graph, &tab.inline_inputs);

            if let Err(e) = crate::node::graph_io::save_graph_definition_to_json(&path, &tab.graph)
            {
                eprintln!("Failed to save graph: {}", e);
                return false;
            }

            // Save hyperparameter values to a separate YAML file in the data directory
            if let Err(e) =
                save_hyperparameter_values(&path, &tab.graph, &tab.hyperparameter_values)
            {
                log::warn!(
                    "[HyperParamStore] Failed to save hyperparameter values: {}",
                    e
                );
            }

            tab.file_path = Some(path.clone());
            tab.title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            tab.is_dirty = false;
            persist_tab_canvas_state(tab);

            if let Some(ui) = ui_handle.upgrade() {
                let active_index = *active_tab_clone.lock().unwrap();
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }

            true
        }
    });

    let active_tab_clone = Arc::clone(&active_tab_index);
    let tabs_clone = Arc::clone(&tabs);
    let save_tab_clone = Arc::clone(&save_tab);
    ui.on_save_json(move || {
        let tab_id = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            tabs_guard.get(active_index).map(|t| t.id)
        };
        if let Some(tab_id) = tab_id {
            let _ = save_tab_clone(tab_id);
        }
    });

    let close_tab_by_id = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let next_untitled_index_clone = Arc::clone(&next_untitled_index);
        let next_tab_id_clone = Arc::clone(&next_tab_id);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let mut active_index = *active_tab_clone.lock().unwrap();
            let remove_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                Some(index) => index,
                None => return,
            };

            if let Some(ui) = ui_handle.upgrade() {
                if remove_index == active_index {
                    sync_active_tab_canvas_state(&ui, &mut tabs_guard, active_index);
                }
                if let Some(tab) = tabs_guard.get(remove_index) {
                    persist_tab_canvas_state(tab);
                }
            }

            tabs_guard.remove(remove_index);

            if tabs_guard.is_empty() {
                let mut next_untitled = next_untitled_index_clone.lock().unwrap();
                let mut next_id = next_tab_id_clone.lock().unwrap();
                tabs_guard.push(new_blank_tab(&mut *next_untitled, &mut *next_id));
                active_index = 0;
            } else if remove_index < active_index {
                active_index -= 1;
            } else if remove_index == active_index && active_index >= tabs_guard.len() {
                active_index = tabs_guard.len() - 1;
            }

            *active_tab_clone.lock().unwrap() = active_index;
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                if let Some(tab) = tabs_guard.get(active_index) {
                    apply_canvas_view_state(&ui, &tab.canvas_view_state);
                }
            }
        }
    });

    let close_tab_by_id_for_request = Arc::clone(&close_tab_by_id);
    let _request_close_tab = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let pending_close_tab_id_for_request = Arc::clone(&pending_close_tab_id);
        let close_tab_by_id_for_request = Arc::clone(&close_tab_by_id_for_request);
        let ui_handle = ui.as_weak();
        move |tab_id: u64| {
            let tabs_guard = tabs_clone.lock().unwrap();
            let tab = match tabs_guard.iter().find(|t| t.id == tab_id) {
                Some(tab) => tab,
                None => return,
            };

            if let Some(ui) = ui_handle.upgrade() {
                if tab.is_running {
                    *pending_close_tab_id_for_request.lock().unwrap() = Some(tab_id);
                    ui.set_show_running_confirm(true);
                    return;
                }

                if tab.is_dirty {
                    *pending_close_tab_id_for_request.lock().unwrap() = Some(tab_id);
                    ui.set_show_save_confirm(true);
                    return;
                }
            }

            close_tab_by_id_for_request(tab_id);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let close_tab_by_id_clone = Arc::clone(&close_tab_by_id);
    ui.on_close_tab(move || {
        let tab_id = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            tabs_guard.get(active_index).map(|tab| tab.id)
        };
        if let Some(tab_id) = tab_id {
            close_tab_by_id_clone(tab_id);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let next_untitled_index_clone = Arc::clone(&next_untitled_index);
    let next_tab_id_clone = Arc::clone(&next_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_new_tab(move || {
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let previous_active = *active_tab_clone.lock().unwrap();
            sync_active_tab_canvas_state(&ui, &mut tabs_guard, previous_active);
            if let Some(tab) = tabs_guard.get(previous_active) {
                persist_tab_canvas_state(tab);
            }

            let mut next_untitled = next_untitled_index_clone.lock().unwrap();
            let mut next_id = next_tab_id_clone.lock().unwrap();
            tabs_guard.push(new_blank_tab(&mut *next_untitled, &mut *next_id));
            let active_index = tabs_guard.len() - 1;
            *active_tab_clone.lock().unwrap() = active_index;
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            if let Some(tab) = tabs_guard.get(active_index) {
                apply_canvas_view_state(&ui, &tab.canvas_view_state);
            }
        }
    });

    #[cfg(target_os = "macos")]
    {
        let ui_weak = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(100), move || {
            install_menu(MenuActions {
                open: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_open_json();
                        }
                    }
                }),
                save: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_save_json();
                        }
                    }
                }),
                save_as: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_save_json_as();
                        }
                    }
                }),
                new_tab: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_new_tab();
                        }
                    }
                }),
                close_tab: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_close_tab();
                        }
                    }
                }),
                quit: Box::new({
                    let ui_weak = ui_weak.clone();
                    move || {
                        if let Some(ui) = ui_weak.upgrade() {
                            ui.invoke_close_tab();
                        }
                    }
                }),
            });
        });
    }

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_select_tab(move |index: i32| {
        if index < 0 {
            return;
        }
        if let Some(ui) = ui_handle.upgrade() {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let index = index as usize;
            if index >= tabs_guard.len() {
                return;
            }
            let previous_active = *active_tab_clone.lock().unwrap();
            sync_active_tab_canvas_state(&ui, &mut tabs_guard, previous_active);
            if let Some(tab) = tabs_guard.get(previous_active) {
                persist_tab_canvas_state(tab);
            }
            *active_tab_clone.lock().unwrap() = index;
            refresh_active_tab_ui(&ui, &tabs_guard, index);
            if let Some(tab) = tabs_guard.get(index) {
                apply_canvas_view_state(&ui, &tab.canvas_view_state);
            }
        }
    });

    let pending_close_tab_id_for_save = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_save = Arc::clone(&close_tab_by_id);
    let save_tab_for_save = Arc::clone(&save_tab);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_save(move || {
        if let Some(tab_id) = pending_close_tab_id_for_save.lock().unwrap().take() {
            if save_tab_for_save(tab_id) {
                close_tab_by_id_for_save(tab_id);
            }
        }
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let pending_close_tab_id_for_discard = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_discard = Arc::clone(&close_tab_by_id);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_discard(move || {
        if let Some(tab_id) = pending_close_tab_id_for_discard.lock().unwrap().take() {
            close_tab_by_id_for_discard(tab_id);
        }
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let pending_close_tab_id_for_cancel = Arc::clone(&pending_close_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_save_confirm_cancel(move || {
        pending_close_tab_id_for_cancel.lock().unwrap().take();
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_save_confirm(false);
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let pending_close_tab_id_for_running = Arc::clone(&pending_close_tab_id);
    let close_tab_by_id_for_running = Arc::clone(&close_tab_by_id);
    let ui_handle = ui.as_weak();
    ui.on_running_confirm_close(move || {
        let tab_id = match pending_close_tab_id_for_running.lock().unwrap().take() {
            Some(tab_id) => tab_id,
            None => return,
        };

        {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.iter_mut().find(|t| t.id == tab_id) {
                if let Some(stop_flag) = tab.stop_flag.as_ref() {
                    info!("请求停止节点图执行");
                    stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }

        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_running_confirm(false);
        }

        let tabs_guard = tabs_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.iter().find(|t| t.id == tab_id) {
            if tab.is_dirty {
                if let Some(ui) = ui_handle.upgrade() {
                    *pending_close_tab_id_for_running.lock().unwrap() = Some(tab_id);
                    ui.set_show_save_confirm(true);
                }
            } else {
                close_tab_by_id_for_running(tab_id);
            }
        }
    });

    let pending_close_tab_id_for_running_cancel = Arc::clone(&pending_close_tab_id);
    let ui_handle = ui.as_weak();
    ui.on_running_confirm_cancel(move || {
        pending_close_tab_id_for_running_cancel
            .lock()
            .unwrap()
            .take();
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_running_confirm(false);
        }
    });

    // --- Save As ---

    let do_save_as = Arc::new({
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let ui_handle = ui.as_weak();
        move |path: PathBuf, tab_id: u64| -> bool {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let tab_index = match tabs_guard.iter().position(|t| t.id == tab_id) {
                Some(index) => index,
                None => return false,
            };

            if let Some(ui) = ui_handle.upgrade() {
                let active_index = *active_tab_clone.lock().unwrap();
                if active_index == tab_index {
                    sync_active_tab_canvas_state(&ui, &mut tabs_guard, active_index);
                }
            }

            let tab = &mut tabs_guard[tab_index];
            apply_inline_inputs_to_graph(&mut tab.graph, &tab.inline_inputs);

            if let Err(e) = crate::node::graph_io::save_graph_definition_to_json(&path, &tab.graph)
            {
                log::error!("Failed to save graph: {}", e);
                return false;
            }

            if let Err(e) =
                save_hyperparameter_values(&path, &tab.graph, &tab.hyperparameter_values)
            {
                log::warn!(
                    "[HyperParamStore] Failed to save hyperparameter values: {}",
                    e
                );
            }

            tab.file_path = Some(path.clone());
            tab.title = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            tab.is_dirty = false;
            persist_tab_canvas_state(tab);

            if let Some(ui) = ui_handle.upgrade() {
                let active_index = *active_tab_clone.lock().unwrap();
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }

            true
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let pending_save_as_for_open = Arc::clone(&pending_save_as);
    let do_save_as_for_open = Arc::clone(&do_save_as);
    let ui_handle = ui.as_weak();
    ui.on_save_json_as(move || {
        let tab_id = {
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            match tabs_guard.get(active_index).map(|t| t.id) {
                Some(id) => id,
                None => return,
            }
        };

        let path = match rfd::FileDialog::new()
            .add_filter("Node Graph", &["json"])
            .set_file_name("node_graph.json")
            .save_file()
        {
            Some(path) => path,
            None => return,
        };

        if path.exists() {
            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            *pending_save_as_for_open.lock().unwrap() = Some((path, tab_id));
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_overwrite_confirm_message(
                    format!("文件 \"{}\" 已存在，是否覆盖？", filename).into(),
                );
                ui.set_show_overwrite_confirm(true);
            }
        } else {
            do_save_as_for_open(path, tab_id);
        }
    });

    let pending_save_as_for_overwrite = Arc::clone(&pending_save_as);
    let do_save_as_for_overwrite = Arc::clone(&do_save_as);
    let ui_handle = ui.as_weak();
    ui.on_overwrite_confirm_overwrite(move || {
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_overwrite_confirm(false);
        }
        if let Some((path, tab_id)) = pending_save_as_for_overwrite.lock().unwrap().take() {
            do_save_as_for_overwrite(path, tab_id);
        }
    });

    let pending_save_as_for_cancel = Arc::clone(&pending_save_as);
    let ui_handle = ui.as_weak();
    ui.on_overwrite_confirm_cancel(move || {
        pending_save_as_for_cancel.lock().unwrap().take();
        if let Some(ui) = ui_handle.upgrade() {
            ui.set_show_overwrite_confirm(false);
        }
    });

    let _ = _request_close_tab;
}
