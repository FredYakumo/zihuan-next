use slint::{ModelRc, VecModel, SharedString, ComponentHandle};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::AtomicBool;

use crate::error::Result;
use crate::node::graph_io::{
    NodeGraphDefinition,
};
use crate::node::registry::NODE_REGISTRY;

use crate::ui::graph_window::{
    NodeGraphWindow, NodeTypeVm,
};
use crate::ui::node_graph_view_callbacks::{
    bind_canvas_callbacks, bind_inline_port_callbacks, bind_message_list_callbacks,
    bind_qq_message_list_callbacks,
    bind_tab_callbacks,
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
        );
        tab.selection.apply_to_ui(ui);
        ui.set_is_graph_running(tab.is_running);
    }
    update_tabs_ui(ui, tabs, active_index);
}

pub fn show_graph(initial_graph: Option<NodeGraphDefinition>) -> Result<()> {
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

    let mut initial_tab = new_blank_tab(&mut next_untitled_index, &mut next_tab_id);
    if let Some(graph) = initial_graph {
        initial_tab.graph = graph.clone();
        initial_tab.inline_inputs = build_inline_inputs_from_graph(&graph);
        initial_tab.is_dirty = false;
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
        .map(|meta| NodeTypeVm {
            type_id: meta.type_id.clone().into(),
            display_name: meta.display_name.clone().into(),
            category: meta.category.clone().into(),
            description: meta.description.clone().into(),
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
    );

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

