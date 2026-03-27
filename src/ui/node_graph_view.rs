use slint::{Model, ModelRc, VecModel, SharedString, ComponentHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use crate::error::Result;
use crate::node::graph_io::{
    NodeGraphDefinition, validate_graph_definition,
};
use crate::ui::graph_window::ValidationIssueVm;
use crate::node::registry::NODE_REGISTRY;

use crate::ui::graph_window::{
    NodeGraphWindow, NodeTypeVm, PortHelpVm, LogEntryVm,
};
use crate::ui::node_graph_view_callbacks::{
    bind_canvas_callbacks, bind_format_string_editor_callbacks, bind_hyperparameter_callbacks,
    bind_inline_port_callbacks, bind_message_list_callbacks,
    bind_qq_message_list_callbacks,
    bind_tab_callbacks,
    bind_tool_editor_callbacks,
    bind_window_callbacks,
};
use crate::ui::node_graph_view_geometry::{
    EDGE_THICKNESS_RATIO, GRID_SIZE,
};
use crate::ui::node_graph_view_inline::build_inline_inputs_from_graph;
use crate::ui::node_graph_view_vm::apply_graph_to_ui;
use crate::ui::selection::SelectionState;
use crate::ui::window_state::{apply_window_state, load_window_state, save_window_state, WindowState};

use crate::ui::node_render::InlinePortValue;

pub(crate) struct GraphTabState {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) graph: NodeGraphDefinition,
    pub(crate) selection: SelectionState,
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,
    /// Hyperparameter values for this graph – stored in a separate YAML file,
    /// not serialised into the node-graph JSON.
    pub(crate) hyperparameter_values: HashMap<String, serde_json::Value>,
    pub(crate) is_dirty: bool,
    pub(crate) is_running: bool,
    pub(crate) stop_flag: Option<Arc<AtomicBool>>,
}

pub(crate) fn tab_display_title(tab: &GraphTabState) -> String {
    if tab.is_dirty {
        format!("{}*", tab.title)
    } else {
        tab.title.clone()
    }
}

pub(crate) fn new_blank_tab(next_untitled: &mut usize, next_id: &mut u64) -> GraphTabState {
    let title = format!("未命名-{}", *next_untitled);
    *next_untitled += 1;
    let id = *next_id;
    *next_id += 1;

    GraphTabState {
        id,
        title,
        file_path: None,
        graph: NodeGraphDefinition::default(),
        selection: SelectionState::default(),
        inline_inputs: HashMap::new(),
        hyperparameter_values: HashMap::new(),
        is_dirty: false,
        is_running: false,
        stop_flag: None,
    }
}

pub(crate) fn update_tabs_ui(ui: &NodeGraphWindow, tabs: &[GraphTabState], active_index: usize) {
    let titles: Vec<SharedString> = tabs.iter().map(|t| tab_display_title(t).into()).collect();
    ui.set_graph_tabs(ModelRc::new(VecModel::from(titles)));
    ui.set_active_tab_index(active_index as i32);
}

pub(crate) fn refresh_active_tab_ui(ui: &NodeGraphWindow, tabs: &[GraphTabState], active_index: usize) {
    if let Some(tab) = tabs.get(active_index) {
        apply_graph_to_ui(
            ui,
            &tab.graph,
            Some(tab_display_title(tab)),
            &tab.selection,
            &tab.inline_inputs,
            &tab.hyperparameter_values,
        );
        tab.selection.apply_to_ui(ui);
        ui.set_is_graph_running(tab.is_running);
    }
    update_tabs_ui(ui, tabs, active_index);
}

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>, graph_file_path: Option<&std::path::Path>) -> Result<()> {
    register_cjk_fonts();

    let ui = NodeGraphWindow::new()
        .map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))?;

    #[cfg(target_os = "macos")]
    ui.set_show_in_window_menu(false);

    if let Some(state) = load_window_state() {
        apply_window_state(&ui.window(), &state);
    }

    let mut next_untitled_index = 1usize;
    let mut next_tab_id = 1u64;

    // Shared state for pending graph open (used both by file-open dialog and CLI startup)
    let pending_open_graph: Arc<Mutex<Option<(PathBuf, NodeGraphDefinition)>>> =
        Arc::new(Mutex::new(None));

    let mut initial_tab = new_blank_tab(&mut next_untitled_index, &mut next_tab_id);
    let mut startup_has_validation_issues = false;
    if let Some(graph) = initial_graph {
        let issues = validate_graph_definition(&graph);
        if issues.is_empty() {
            // No issues – load directly
            initial_tab.graph = graph.clone();
            initial_tab.inline_inputs = build_inline_inputs_from_graph(&graph);
            if let Some(path) = graph_file_path {
                initial_tab.hyperparameter_values =
                    crate::util::hyperparam_store::load_hyperparameter_values(path);
                initial_tab.file_path = Some(path.to_path_buf());
                initial_tab.title = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
            }
            initial_tab.is_dirty = false;
        } else {
            // Validation issues found – store as pending and show dialog after startup
            let pending_path = graph_file_path
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("unknown"));
            *pending_open_graph.lock().unwrap() = Some((pending_path, graph));
            startup_has_validation_issues = true;
        }
    }

    let tabs = Arc::new(Mutex::new(vec![initial_tab]));
    let active_tab_index = Arc::new(Mutex::new(0usize));
    let next_untitled_index = Arc::new(Mutex::new(next_untitled_index));
    let next_tab_id = Arc::new(Mutex::new(next_tab_id));
    let pending_close_tab_id: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    // Load available node types from registry
    let node_types: Vec<NodeTypeVm> = NODE_REGISTRY
        .get_all_types()
        .iter()
        .map(|meta| {
            let (input_ports, output_ports) = NODE_REGISTRY
                .get_node_ports(&meta.type_id)
                .unwrap_or_default();

            let input_help: Vec<PortHelpVm> = input_ports
                .iter()
                .map(|p| PortHelpVm {
                    name: p.name.clone().into(),
                    data_type: p.data_type.to_string().into(),
                    description: p.description.clone().unwrap_or_default().into(),
                    required: p.required,
                    connection_text: SharedString::default(),
                })
                .collect();

            let output_help: Vec<PortHelpVm> = output_ports
                .iter()
                .map(|p| PortHelpVm {
                    name: p.name.clone().into(),
                    data_type: p.data_type.to_string().into(),
                    description: p.description.clone().unwrap_or_default().into(),
                    required: false,
                    connection_text: SharedString::default(),
                })
                .collect();

            NodeTypeVm {
                type_id: meta.type_id.clone().into(),
                display_name: meta.display_name.clone().into(),
                category: meta.category.clone().into(),
                description: meta.description.clone().into(),
                input_ports: ModelRc::new(VecModel::from(input_help)),
                output_ports: ModelRc::new(VecModel::from(output_help)),
            }
        })
        .collect();

    let mut categories: Vec<SharedString> = node_types
        .iter()
        .map(|n| n.category.clone())
        .collect::<Vec<_>>();
    categories.sort();
    categories.dedup();
    
    ui.set_node_categories(ModelRc::new(VecModel::from(categories)));
    ui.set_available_node_types(ModelRc::new(VecModel::from(node_types.clone())));
    
    let all_node_types = Arc::new(node_types);
    ui.set_grid_size(GRID_SIZE);
    ui.set_edge_thickness(GRID_SIZE * EDGE_THICKNESS_RATIO);

    {
        let tabs_guard = tabs.lock().unwrap();
        let active_index = *active_tab_index.lock().unwrap();
        refresh_active_tab_ui(&ui, &tabs_guard, active_index);
    }

    bind_tab_callbacks(
        &ui,
        Arc::clone(&tabs),
        Arc::clone(&active_tab_index),
        Arc::clone(&next_untitled_index),
        Arc::clone(&next_tab_id),
        Arc::clone(&pending_close_tab_id),
        Arc::clone(&pending_open_graph),
    );

    // If startup graph had validation issues, show the dialog once the event loop is running
    if startup_has_validation_issues {
        let ui_weak = ui.as_weak();
        let pending_clone = Arc::clone(&pending_open_graph);
        slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
            let guard = pending_clone.lock().unwrap();
            if let Some((_, ref graph)) = *guard {
                let issues = validate_graph_definition(graph);
                let issue_vms: Vec<ValidationIssueVm> = issues
                    .iter()
                    .map(|i| ValidationIssueVm {
                        severity: i.severity.clone().into(),
                        message: i.message.clone().into(),
                    })
                    .collect();
                drop(guard);
                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_validation_issues(slint::ModelRc::from(
                        std::rc::Rc::new(slint::VecModel::from(issue_vms)),
                    ));
                    ui.set_show_validation_fix_dialog(true);
                }
            }
        });
    }

    bind_window_callbacks(
        &ui,
        Arc::clone(&tabs),
        Arc::clone(&active_tab_index),
        Arc::clone(&all_node_types),
    );

    bind_canvas_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_inline_port_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_message_list_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_qq_message_list_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_hyperparameter_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_tool_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_format_string_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));

    // Log history dialog callbacks
    {
        let ui_weak = ui.as_weak();
        ui.on_open_log_history(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let history = crate::ui::log_overlay::get_history();
                let vms: Vec<LogEntryVm> = history
                    .iter()
                    .map(|e| LogEntryVm {
                        level: e.level.to_string().into(),
                        message: e.message.clone().into(),
                    })
                    .collect();
                ui.set_log_history(ModelRc::new(VecModel::from(vms)));
                ui.set_show_log_history(true);
            }
        });
    }
    {
        let ui_weak = ui.as_weak();
        ui.on_close_log_history(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_show_log_history(false);
            }
        });
    }

    // Log overlay: poll the ring buffer every 100 ms and update the Slint model
    let last_log_time: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
    let last_log_time_clone = Rc::clone(&last_log_time);
    let ui_weak_log = ui.as_weak();
    let _poll_timer = {
        let t = slint::Timer::default();
        t.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(100),
            move || {
                let new_entries = crate::ui::log_overlay::drain_new_entries();
                if !new_entries.is_empty() {
                    last_log_time_clone.set(Some(Instant::now()));
                    if let Some(ui) = ui_weak_log.upgrade() {
                        let existing: Vec<LogEntryVm> = ui.get_log_entries().iter().collect();
                        let mut all = existing;
                        for e in new_entries {
                            all.push(LogEntryVm {
                                level: e.level.to_string().into(),
                                message: e.message.into(),
                            });
                        }
                        if all.len() > 5 {
                            let drain_count = all.len() - 5;
                            all.drain(0..drain_count);
                        }
                        ui.set_log_entries(ModelRc::new(VecModel::from(all)));
                        ui.set_log_overlay_opacity(1.0);
                    }
                } else if let Some(last) = last_log_time_clone.get() {
                    let elapsed = last.elapsed();
                    if elapsed >= std::time::Duration::from_secs(5) {
                        if let Some(ui) = ui_weak_log.upgrade() {
                            if ui.get_log_overlay_opacity() > 0.001 {
                                ui.set_log_overlay_opacity(0.0);
                            }
                        }
                    }
                    // Clear entries after fade completes (~5.6 s total)
                    if elapsed >= std::time::Duration::from_millis(5600) {
                        if let Some(ui) = ui_weak_log.upgrade() {
                            ui.set_log_entries(ModelRc::new(VecModel::from(Vec::<LogEntryVm>::new())));
                        }
                        last_log_time_clone.set(None);
                    }
                }
            },
        );
        t
    };

    let run_result = ui.run();
    if run_result.is_ok() {
        let state = WindowState::from_window(&ui.window());
        if let Err(e) = save_window_state(&state) {
            eprintln!("Failed to save window state: {e}");
        }
    }

    run_result.map_err(|e| crate::error::Error::StringError(format!("UI error: {e}")))
}

fn register_cjk_fonts() {
    use slint::fontique_07::{fontique, shared_collection};
    use std::sync::Arc;

    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/Library/Fonts/NotoSansCJKsc-Regular.otf",
        "/Library/Fonts/NotoSansCJK-Regular.ttc",
    ];

    let mut collection = shared_collection();

    for path in candidates {
        if !Path::new(path).exists() {
            continue;
        }

        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };

        let blob = fontique::Blob::new(Arc::new(bytes));
        let fonts = collection.register_fonts(blob, None);
        if fonts.is_empty() {
            continue;
        }

        let ids: Vec<_> = fonts.iter().map(|font| font.0).collect();
        let hani = fontique::FallbackKey::new("Hani", None);
        let hira = fontique::FallbackKey::new("Hira", None);
        let kana = fontique::FallbackKey::new("Kana", None);

        collection.append_fallbacks(hani, ids.iter().copied());
        collection.append_fallbacks(hira, ids.iter().copied());
        collection.append_fallbacks(kana, ids.iter().copied());
    }
}

