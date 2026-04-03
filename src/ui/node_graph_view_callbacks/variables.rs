use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, VecModel};

use zihuan_llm::brain_tool::BrainToolDefinition;
use zihuan_node::data_value::DataType;
use zihuan_node::function_graph::{embedded_function_config_from_node, FUNCTION_CONFIG_PORT};
use zihuan_node::graph_io::{GraphVariable, NodeGraphDefinition, PortBinding, PortBindingKind};
use zihuan_node::util::set_variable::{SET_VARIABLE_NAME_PORT, SET_VARIABLE_TYPE_PORT};
use crate::ui::graph_window::{GraphVariableVm, HyperParameterVm, NodeGraphWindow};
use crate::ui::node_graph_view::{refresh_active_tab_ui, GraphTabState};

fn parse_variable_data_type(type_str: &str) -> DataType {
    match type_str {
        "Integer" => DataType::Integer,
        "Float" => DataType::Float,
        "Boolean" => DataType::Boolean,
        "Password" => DataType::Password,
        _ => DataType::String,
    }
}

fn str_to_json_value(type_str: &str, value_str: &str) -> Option<serde_json::Value> {
    if value_str.is_empty() {
        return None;
    }
    match type_str {
        "Boolean" => Some(serde_json::Value::Bool(
            value_str.eq_ignore_ascii_case("true"),
        )),
        "Integer" => value_str
            .parse::<i64>()
            .ok()
            .map(|n| serde_json::Value::Number(n.into())),
        "Float" => value_str
            .parse::<f64>()
            .ok()
            .and_then(|n| serde_json::Number::from_f64(n).map(serde_json::Value::Number)),
        _ => Some(serde_json::Value::String(value_str.to_string())),
    }
}

fn variable_value_to_string(value: Option<&serde_json::Value>) -> String {
    match value {
        Some(serde_json::Value::String(value)) => value.clone(),
        Some(serde_json::Value::Bool(value)) => value.to_string(),
        Some(serde_json::Value::Number(value)) => value.to_string(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn is_type_compatible(variable_type: &DataType, port_type: &DataType) -> bool {
    match (variable_type, port_type) {
        (a, b) if a == b => true,
        (DataType::String, DataType::Password) => true,
        (DataType::Password, DataType::String) => true,
        _ => false,
    }
}

fn clear_set_variable_config_if_matches(node: &mut zihuan_node::graph_io::NodeDefinition, name: &str) {
    if node.node_type != "set_variable" {
        return;
    }

    let selected_name = node
        .inline_values
        .get(SET_VARIABLE_NAME_PORT)
        .and_then(|value| value.as_str());
    if selected_name == Some(name) {
        node.inline_values.remove(SET_VARIABLE_NAME_PORT);
        node.inline_values.remove(SET_VARIABLE_TYPE_PORT);
    }
}

fn remove_variable_references_from_graph(graph: &mut NodeGraphDefinition, name: &str) {
    for node in &mut graph.nodes {
        node.port_bindings.retain(|_, binding| {
            !(binding.kind == PortBindingKind::Variable && binding.name == name)
        });
        clear_set_variable_config_if_matches(node, name);

        if let Some(mut config) = embedded_function_config_from_node(node) {
            remove_variable_references_from_graph(&mut config.subgraph, name);
            if let Ok(value) = serde_json::to_value(&config) {
                node.inline_values
                    .insert(FUNCTION_CONFIG_PORT.to_string(), value);
            }
        }

        if let Some(tools_value) = node.inline_values.get("tools_config").cloned() {
            if let Ok(mut tools) = serde_json::from_value::<Vec<BrainToolDefinition>>(tools_value) {
                for tool in &mut tools {
                    remove_variable_references_from_graph(&mut tool.subgraph, name);
                }
                if let Ok(value) = serde_json::to_value(&tools) {
                    node.inline_values.insert("tools_config".to_string(), value);
                }
            }
        }
    }

    zihuan_node::graph_io::refresh_port_types(graph);
}

fn set_set_variable_selection(
    graph: &mut NodeGraphDefinition,
    node_id: &str,
    variable_name: &str,
    variable_type: &DataType,
) -> bool {
    let Some(node) = graph.nodes.iter_mut().find(|node| node.id == node_id) else {
        return false;
    };
    if node.node_type != "set_variable" {
        return false;
    }

    node.inline_values.insert(
        SET_VARIABLE_NAME_PORT.to_string(),
        serde_json::Value::String(variable_name.to_string()),
    );
    node.inline_values.insert(
        SET_VARIABLE_TYPE_PORT.to_string(),
        serde_json::Value::String(variable_type.to_string()),
    );
    zihuan_node::graph_io::refresh_port_types(graph);
    true
}

pub(crate) fn bind_variable_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    {
        let ui_handle = ui.as_weak();
        ui.on_open_variable_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_variable_manager(true);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_variable_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_variable_manager(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_add_graph_variable(move |name, data_type, initial_value| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if tab.root_graph().variables.iter().any(|variable| variable.name == name) {
                    return;
                }

                let data_type = parse_variable_data_type(data_type.as_str());
                let initial_value = str_to_json_value(data_type.to_string().as_str(), initial_value.as_str());
                tab.root_graph_mut().variables.push(GraphVariable {
                    name,
                    data_type,
                    initial_value,
                });
                if !tab.is_subgraph_page() {
                    tab.load_current_page_into_mirror();
                }
                tab.is_dirty = true;
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_set_graph_variable_initial_value(move |name, initial_value| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(variable) = tab
                    .root_graph_mut()
                    .variables
                    .iter_mut()
                    .find(|variable| variable.name == name.as_str())
                {
                    variable.initial_value =
                        str_to_json_value(variable.data_type.to_string().as_str(), initial_value.as_str());
                    tab.is_dirty = true;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_delete_graph_variable(move |name| {
            let name = name.as_str().to_string();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.root_graph_mut().variables.retain(|variable| variable.name != name);
                remove_variable_references_from_graph(tab.root_graph_mut(), &name);
                remove_variable_references_from_graph(tab.graph_mut(), &name);
                tab.commit_current_page_to_parent();
                tab.is_dirty = true;
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_set_variable_node_selected(move |node_id, variable_name| {
            let variable_name = variable_name.as_str().to_string();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let selected_type = tab
                    .root_graph()
                    .variables
                    .iter()
                    .find(|variable| variable.name == variable_name)
                    .map(|variable| variable.data_type.clone());
                let Some(selected_type) = selected_type else {
                    return;
                };

                if set_set_variable_selection(tab.graph_mut(), node_id.as_str(), &variable_name, &selected_type) {
                    tab.commit_current_page_to_parent();
                    tab.is_dirty = true;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_port_right_clicked(move |node_id, port_name, x, y| {
            let ui = match ui_handle.upgrade() {
                Some(ui) => ui,
                None => return,
            };

            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let Some(tab) = tabs_guard.get(active_index) else {
                return;
            };

            let port_type = tab
                .graph()
                .nodes
                .iter()
                .find(|node| node.id == node_id.as_str())
                .and_then(|node| {
                    node.input_ports
                        .iter()
                        .find(|port| port.name == port_name.as_str())
                        .map(|port| port.data_type.clone())
                });
            let Some(port_type) = port_type else {
                return;
            };

            let current_binding = tab
                .graph()
                .nodes
                .iter()
                .find(|node| node.id == node_id.as_str())
                .and_then(|node| node.port_bindings.get(port_name.as_str()))
                .cloned();

            let variables: Vec<GraphVariableVm> = tab
                .root_graph()
                .variables
                .iter()
                .filter(|variable| is_type_compatible(&variable.data_type, &port_type))
                .map(|variable| GraphVariableVm {
                    name: variable.name.clone().into(),
                    data_type: variable.data_type.to_string().into(),
                    initial_value: variable_value_to_string(variable.initial_value.as_ref()).into(),
                })
                .collect();

            let hyperparameters: Vec<HyperParameterVm> = tab
                .root_graph()
                .hyperparameters
                .iter()
                .filter(|hp| is_type_compatible(&hp.data_type, &port_type))
                .map(|hp| HyperParameterVm {
                    name: hp.name.clone().into(),
                    group: hp.group.clone().into(),
                    data_type: hp.data_type.to_string().into(),
                    value: tab
                        .hyperparameter_values
                        .get(&hp.name)
                        .map(|value| variable_value_to_string(Some(value)))
                        .unwrap_or_default()
                        .into(),
                    required: hp.required,
                    description: hp.description.clone().unwrap_or_default().into(),
                })
                .collect();

            ui.set_port_bind_node_id(node_id.clone());
            ui.set_port_bind_port_name(port_name.clone());
            ui.set_port_bind_current_binding(
                current_binding
                    .as_ref()
                    .map(|binding| binding.name.clone())
                    .unwrap_or_default()
                    .into(),
            );
            ui.set_port_bind_current_kind(
                current_binding
                    .map(|binding| match binding.kind {
                        PortBindingKind::Hyperparameter => "hyperparameter",
                        PortBindingKind::Variable => "variable",
                    })
                    .unwrap_or_default()
                    .into(),
            );
            ui.set_port_bind_compatible_variables(ModelRc::new(VecModel::from(variables)));
            ui.set_port_bind_compatible_params(ModelRc::new(VecModel::from(hyperparameters)));
            ui.set_port_bind_menu_x(x);
            ui.set_port_bind_menu_y(y);
            ui.set_show_port_bind_menu(true);
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_port_bind_menu(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_bind_menu(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_open_port_bind_variable_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_bind_menu(false);
                ui.set_show_port_variable_bind_dialog(true);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_open_port_bind_hyperparameter_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_bind_menu(false);
                ui.set_show_port_bind_dialog(true);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_bind_port_variable(move |node_id, port_name, variable_name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab
                    .graph_mut()
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == node_id.as_str())
                {
                    node.port_bindings.insert(
                        port_name.to_string(),
                        PortBinding::variable(variable_name.to_string()),
                    );
                    tab.commit_current_page_to_parent();
                    tab.is_dirty = true;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_show_port_variable_bind_dialog(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_bind_port_hyperparameter(move |node_id, port_name, hp_name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab
                    .graph_mut()
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == node_id.as_str())
                {
                    node.port_bindings.insert(
                        port_name.to_string(),
                        PortBinding::hyperparameter(hp_name.to_string()),
                    );
                    tab.commit_current_page_to_parent();
                    tab.is_dirty = true;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_show_port_bind_dialog(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_unbind_port_hyperparameter(move |node_id, port_name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab
                    .graph_mut()
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == node_id.as_str())
                {
                    node.port_bindings.remove(port_name.as_str());
                    tab.commit_current_page_to_parent();
                    tab.is_dirty = true;
                }
            }

            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_show_port_bind_dialog(false);
                ui.set_show_port_variable_bind_dialog(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_port_bind_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_bind_dialog(false);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_port_variable_bind_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_variable_bind_dialog(false);
            }
        });
    }
}
