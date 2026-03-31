use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::error::Result;
use crate::llm::brain_tool::BrainToolDefinition;
use crate::node::function_graph::{
    default_embedded_function_config, embedded_function_config_from_node,
    sync_function_node_definition, sync_function_subgraph_signature, FUNCTION_CONFIG_PORT,
};
use crate::node::graph_io::{validate_graph_definition, NodeGraphDefinition};
use crate::node::registry::NODE_REGISTRY;
use crate::ui::graph_window::ValidationIssueVm;

use crate::ui::canvas_state::{load_canvas_view_state, save_canvas_view_state, CanvasViewState};
use crate::ui::graph_window::{LogEntryVm, NodeGraphWindow, NodeTypeVm, PortHelpVm};
use crate::ui::node_graph_view_callbacks::{
    bind_canvas_callbacks, bind_format_string_editor_callbacks, bind_function_editor_callbacks,
    bind_hyperparameter_callbacks, bind_inline_port_callbacks,
    bind_json_extract_editor_callbacks, bind_message_list_callbacks,
    bind_qq_message_list_callbacks, bind_tab_callbacks, bind_tool_editor_callbacks,
    bind_window_callbacks,
};
use crate::ui::node_graph_view_clipboard::NodeClipboard;
use crate::ui::node_graph_view_geometry::{EDGE_THICKNESS_RATIO, GRID_SIZE};
use crate::ui::node_graph_view_inline::build_inline_inputs_from_graph;
use crate::ui::node_graph_view_vm::apply_graph_to_ui;
use crate::ui::selection::SelectionState;
use crate::ui::window_state::{
    apply_window_state, load_window_state, save_window_state, WindowState,
};

use crate::ui::node_render::InlinePortValue;

pub(crate) fn collect_hyperparameter_groups(graph: &NodeGraphDefinition) -> Vec<String> {
    let mut groups = graph.hyperparameter_groups.clone();
    for hp in &graph.hyperparameters {
        if !groups.iter().any(|group| group == &hp.group) {
            groups.push(hp.group.clone());
        }
    }
    if !groups.iter().any(|group| group == "default") {
        groups.insert(0, "default".to_string());
    }
    groups.retain(|group| !group.trim().is_empty());
    groups.sort();
    groups.dedup();
    if let Some(index) = groups.iter().position(|group| group == "default") {
        if index != 0 {
            let default_group = groups.remove(index);
            groups.insert(0, default_group);
        }
    }
    groups
}

pub(crate) struct GraphTabState {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) root_page: GraphPageState,
    pub(crate) page_stack: Vec<GraphPageState>,
    pub(crate) graph: NodeGraphDefinition,
    pub(crate) selection: SelectionState,
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,
    pub(crate) canvas_view_state: CanvasViewState,
    /// Hyperparameter values for this graph – stored in a separate YAML file,
    /// not serialised into the node-graph JSON.
    pub(crate) hyperparameter_values: HashMap<String, serde_json::Value>,
    pub(crate) is_dirty: bool,
    pub(crate) is_running: bool,
    pub(crate) stop_flag: Option<Arc<AtomicBool>>,
}

#[derive(Clone)]
pub(crate) struct GraphPageState {
    pub(crate) owner: Option<SubgraphOwner>,
    pub(crate) title: String,
    pub(crate) graph: NodeGraphDefinition,
    pub(crate) selection: SelectionState,
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,
    pub(crate) canvas_view_state: CanvasViewState,
}

#[derive(Clone)]
pub(crate) enum SubgraphOwner {
    FunctionNode { node_id: String },
    BrainTool { node_id: String, tool_id: String },
}

impl GraphPageState {
    fn new_root(graph: NodeGraphDefinition, canvas_view_state: CanvasViewState) -> Self {
        Self {
            owner: None,
            title: "主图".to_string(),
            inline_inputs: build_inline_inputs_from_graph(&graph),
            graph,
            selection: SelectionState::default(),
            canvas_view_state,
        }
    }

    fn new_child(owner: SubgraphOwner, title: impl Into<String>, graph: NodeGraphDefinition) -> Self {
        Self {
            owner: Some(owner),
            title: title.into(),
            inline_inputs: build_inline_inputs_from_graph(&graph),
            graph,
            selection: SelectionState::default(),
            canvas_view_state: CanvasViewState::default(),
        }
    }
}

impl GraphTabState {
    pub(crate) fn sync_current_page_from_mirror(&mut self) {
        if let Some(page) = self.page_stack.last_mut() {
            page.graph = self.graph.clone();
            page.selection = self.selection.clone();
            page.inline_inputs = self.inline_inputs.clone();
            page.canvas_view_state = self.canvas_view_state.clone();
        } else {
            self.root_page.graph = self.graph.clone();
            self.root_page.selection = self.selection.clone();
            self.root_page.inline_inputs = self.inline_inputs.clone();
            self.root_page.canvas_view_state = self.canvas_view_state.clone();
        }
    }

    pub(crate) fn load_current_page_into_mirror(&mut self) {
        let page = self.page_stack.last().unwrap_or(&self.root_page).clone();
        self.graph = page.graph;
        self.selection = page.selection;
        self.inline_inputs = page.inline_inputs;
        self.canvas_view_state = page.canvas_view_state;
    }

    pub(crate) fn current_page(&self) -> &GraphPageState {
        self.page_stack.last().unwrap_or(&self.root_page)
    }

    pub(crate) fn current_page_mut(&mut self) -> &mut GraphPageState {
        self.page_stack.last_mut().unwrap_or(&mut self.root_page)
    }

    pub(crate) fn graph(&self) -> &NodeGraphDefinition {
        &self.graph
    }

    pub(crate) fn graph_mut(&mut self) -> &mut NodeGraphDefinition {
        &mut self.graph
    }

    pub(crate) fn selection(&self) -> &SelectionState {
        &self.selection
    }

    pub(crate) fn selection_mut(&mut self) -> &mut SelectionState {
        &mut self.selection
    }

    pub(crate) fn inline_inputs(&self) -> &HashMap<String, InlinePortValue> {
        &self.inline_inputs
    }

    pub(crate) fn inline_inputs_mut(&mut self) -> &mut HashMap<String, InlinePortValue> {
        &mut self.inline_inputs
    }

    pub(crate) fn canvas_view_state(&self) -> &CanvasViewState {
        &self.canvas_view_state
    }

    pub(crate) fn canvas_view_state_mut(&mut self) -> &mut CanvasViewState {
        &mut self.canvas_view_state
    }

    pub(crate) fn root_graph(&self) -> &NodeGraphDefinition {
        &self.root_page.graph
    }

    pub(crate) fn root_graph_mut(&mut self) -> &mut NodeGraphDefinition {
        &mut self.root_page.graph
    }

    pub(crate) fn is_subgraph_page(&self) -> bool {
        !self.page_stack.is_empty()
    }

    pub(crate) fn current_page_title(&self) -> String {
        self.current_page().title.clone()
    }

    pub(crate) fn commit_current_page_inline_inputs(&mut self) {
        crate::ui::node_graph_view_inline::apply_inline_inputs_to_graph(
            &mut self.graph,
            &self.inline_inputs,
        );
        self.sync_current_page_from_mirror();
    }

    pub(crate) fn commit_all_pages_to_root(&mut self) {
        self.sync_current_page_from_mirror();
        crate::ui::node_graph_view_inline::apply_inline_inputs_to_graph(
            &mut self.root_page.graph,
            &self.root_page.inline_inputs,
        );
        for page in &mut self.page_stack {
            crate::ui::node_graph_view_inline::apply_inline_inputs_to_graph(
                &mut page.graph,
                &page.inline_inputs,
            );
        }

        for index in (0..self.page_stack.len()).rev() {
            let owner = self.page_stack[index]
                .owner
                .clone()
                .expect("subgraph page should have owner");
            let child_graph = self.page_stack[index].graph.clone();

            if index == 0 {
                embed_subgraph_into_page(&mut self.root_page, owner, child_graph);
            } else {
                embed_subgraph_into_page(&mut self.page_stack[index - 1], owner, child_graph);
            }
        }
    }

    pub(crate) fn pop_subgraph_page(&mut self) -> bool {
        self.commit_current_page_inline_inputs();
        let Some(child_page) = self.page_stack.pop() else {
            return false;
        };
        let Some(owner) = child_page.owner else {
            return false;
        };

        if let Some(parent_page) = self.page_stack.last_mut() {
            embed_subgraph_into_page(parent_page, owner, child_page.graph);
        } else {
            embed_subgraph_into_page(&mut self.root_page, owner, child_page.graph);
        }

        self.load_current_page_into_mirror();

        true
    }

    pub(crate) fn push_subgraph_page(
        &mut self,
        owner: SubgraphOwner,
        title: impl Into<String>,
        graph: NodeGraphDefinition,
    ) {
        self.sync_current_page_from_mirror();
        self.page_stack
            .push(GraphPageState::new_child(owner, title, graph));
        self.load_current_page_into_mirror();
    }

    pub(crate) fn return_to_root_page(&mut self) -> bool {
        if self.page_stack.is_empty() {
            return false;
        }
        self.commit_all_pages_to_root();
        self.page_stack.clear();
        self.load_current_page_into_mirror();
        true
    }

    pub(crate) fn breadcrumb_current_label(&self) -> String {
        self.page_stack
            .last()
            .map(|page| page.title.clone())
            .unwrap_or_else(|| "主图".to_string())
    }
}

fn embed_subgraph_into_page(
    page: &mut GraphPageState,
    owner: SubgraphOwner,
    child_graph: NodeGraphDefinition,
) {
    use crate::ui::node_render::inline_port_key;

    match owner {
        SubgraphOwner::FunctionNode { node_id } => {
            let Some(node) = page.graph.nodes.iter_mut().find(|node| node.id == node_id) else {
                return;
            };
            let mut config = embedded_function_config_from_node(node)
                .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));
            config.subgraph = child_graph;
            sync_function_node_definition(node, &config);
            if let Ok(value) = serde_json::to_value(&config) {
                page.inline_inputs.insert(
                    inline_port_key(&node.id, FUNCTION_CONFIG_PORT),
                    InlinePortValue::Json(value),
                );
            }
        }
        SubgraphOwner::BrainTool { node_id, tool_id } => {
            let Some(node) = page.graph.nodes.iter_mut().find(|node| node.id == node_id) else {
                return;
            };
            let Some(value) = node.inline_values.get("tools_config").cloned() else {
                return;
            };
            let Ok(mut tools) = serde_json::from_value::<Vec<BrainToolDefinition>>(value) else {
                return;
            };
            for tool in &mut tools {
                if tool.id == tool_id {
                    tool.subgraph = child_graph.clone();
                    let input_signature = tool.input_signature();
                    sync_function_subgraph_signature(
                        &mut tool.subgraph,
                        &input_signature,
                        &tool.outputs,
                    );
                }
            }
            if let Ok(value) = serde_json::to_value(&tools) {
                node.inline_values.insert("tools_config".to_string(), value.clone());
                page.inline_inputs.insert(
                    inline_port_key(&node.id, "tools_config"),
                    InlinePortValue::Json(value),
                );
            }
        }
    }
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
        root_page: GraphPageState::new_root(NodeGraphDefinition::default(), CanvasViewState::default()),
        page_stack: Vec::new(),
        graph: NodeGraphDefinition::default(),
        selection: SelectionState::default(),
        inline_inputs: HashMap::new(),
        canvas_view_state: CanvasViewState::default(),
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

pub(crate) fn refresh_active_tab_ui(
    ui: &NodeGraphWindow,
    tabs: &[GraphTabState],
    active_index: usize,
) {
    ui.set_show_graph_context_menu(false);
    if let Some(tab) = tabs.get(active_index) {
        apply_graph_to_ui(
            ui,
            tab.graph(),
            Some(tab_display_title(tab)),
            tab.selection(),
            tab.inline_inputs(),
            &tab.hyperparameter_values,
        );
        let groups = collect_hyperparameter_groups(tab.root_graph());
        ui.set_hyperparameter_groups(ModelRc::new(VecModel::from(
            groups
                .iter()
                .cloned()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )));
        let selected_group = ui.get_selected_hyperparameter_group().to_string();
        let next_group = if groups.iter().any(|group| group == &selected_group) {
            selected_group
        } else {
            "default".to_string()
        };
        ui.set_selected_hyperparameter_group(next_group.into());
        tab.selection().apply_to_ui(ui);
        ui.set_is_graph_running(tab.is_running);
        ui.set_is_subgraph_page(tab.is_subgraph_page());
        ui.set_subgraph_current_label(tab.breadcrumb_current_label().into());
    } else {
        ui.set_is_subgraph_page(false);
        ui.set_subgraph_current_label("".into());
    }
    update_tabs_ui(ui, tabs, active_index);
}

fn persist_window_state(window: &slint::Window) {
    let state = WindowState::from_window(window);
    if let Err(e) = save_window_state(&state) {
        eprintln!("Failed to save window state: {e}");
    }
}

pub(crate) fn capture_canvas_view_state(ui: &NodeGraphWindow) -> CanvasViewState {
    CanvasViewState {
        pan_x: ui.get_canvas_pan_x(),
        pan_y: ui.get_canvas_pan_y(),
        zoom: ui.get_canvas_zoom(),
    }
}

pub(crate) fn apply_canvas_view_state(ui: &NodeGraphWindow, state: &CanvasViewState) {
    ui.set_canvas_pan_x(state.pan_x);
    ui.set_canvas_pan_y(state.pan_y);
    ui.set_canvas_zoom(state.zoom.max(0.2));
}

pub(crate) fn sync_active_tab_canvas_state(
    ui: &NodeGraphWindow,
    tabs: &mut [GraphTabState],
    active_index: usize,
) {
    if let Some(tab) = tabs.get_mut(active_index) {
        *tab.canvas_view_state_mut() = capture_canvas_view_state(ui);
        tab.sync_current_page_from_mirror();
    }
}

pub(crate) fn persist_tab_canvas_state(tab: &GraphTabState) {
    let Some(path) = tab.file_path.as_ref() else {
        return;
    };

    if let Err(e) = save_canvas_view_state(path, &tab.root_page.canvas_view_state) {
        eprintln!("Failed to save canvas state for {}: {e}", path.display());
    }
}

fn persist_all_canvas_states(
    ui: &NodeGraphWindow,
    tabs: &mut [GraphTabState],
    active_index: usize,
) {
    sync_active_tab_canvas_state(ui, tabs, active_index);
    for tab in tabs.iter() {
        persist_tab_canvas_state(tab);
    }
}

pub fn show_graph(
    initial_graph: Option<NodeGraphDefinition>,
    graph_file_path: Option<&std::path::Path>,
    initial_graph_dirty: bool,
) -> Result<()> {
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
            initial_tab.root_page.graph = graph.clone();
            initial_tab.root_page.inline_inputs = build_inline_inputs_from_graph(&graph);
            initial_tab.graph = graph.clone();
            initial_tab.inline_inputs = build_inline_inputs_from_graph(&graph);
            if let Some(path) = graph_file_path {
                initial_tab.hyperparameter_values =
                    crate::util::hyperparam_store::load_hyperparameter_values(
                        path,
                        &initial_tab.root_page.graph,
                    );
                initial_tab.root_page.canvas_view_state =
                    load_canvas_view_state(path).unwrap_or_default();
                initial_tab.canvas_view_state = initial_tab.root_page.canvas_view_state.clone();
                initial_tab.file_path = Some(path.to_path_buf());
                initial_tab.title = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
            }
            initial_tab.is_dirty = initial_graph_dirty;
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

    {
        let ui_weak = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_index_clone = Arc::clone(&active_tab_index);
        ui.window().on_close_requested(move || {
            if let Some(ui) = ui_weak.upgrade() {
                let active_index = *active_tab_index_clone.lock().unwrap();
                let mut tabs_guard = tabs_clone.lock().unwrap();
                persist_all_canvas_states(&ui, &mut tabs_guard, active_index);
                persist_window_state(&ui.window());
            }
            slint::CloseRequestResponse::HideWindow
        });
    }

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
    let node_clipboard: Arc<Mutex<Option<NodeClipboard>>> = Arc::new(Mutex::new(None));
    let last_context_canvas_pos: Arc<Mutex<Option<(f32, f32)>>> = Arc::new(Mutex::new(None));
    let pending_add_node_pos: Arc<Mutex<Option<(f32, f32)>>> = Arc::new(Mutex::new(None));
    ui.set_grid_size(GRID_SIZE);
    ui.set_edge_thickness(GRID_SIZE * EDGE_THICKNESS_RATIO);

    {
        let tabs_guard = tabs.lock().unwrap();
        let active_index = *active_tab_index.lock().unwrap();
        refresh_active_tab_ui(&ui, &tabs_guard, active_index);
        if let Some(tab) = tabs_guard.get(active_index) {
            apply_canvas_view_state(&ui, tab.canvas_view_state());
        }
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
                    ui.set_validation_issues(slint::ModelRc::from(std::rc::Rc::new(
                        slint::VecModel::from(issue_vms),
                    )));
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
        Arc::clone(&node_clipboard),
        Arc::clone(&last_context_canvas_pos),
        Arc::clone(&pending_add_node_pos),
    );

    bind_canvas_callbacks(
        &ui,
        Arc::clone(&tabs),
        Arc::clone(&active_tab_index),
        Arc::clone(&node_clipboard),
        Arc::clone(&last_context_canvas_pos),
    );
    bind_inline_port_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_message_list_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_qq_message_list_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_hyperparameter_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_tool_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_function_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_format_string_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));
    bind_json_extract_editor_callbacks(&ui, Arc::clone(&tabs), Arc::clone(&active_tab_index));

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
                            ui.set_log_entries(ModelRc::new(VecModel::from(
                                Vec::<LogEntryVm>::new(),
                            )));
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
        let active_index = *active_tab_index.lock().unwrap();
        let mut tabs_guard = tabs.lock().unwrap();
        persist_all_canvas_states(&ui, &mut tabs_guard, active_index);
        persist_window_state(&ui.window());
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
