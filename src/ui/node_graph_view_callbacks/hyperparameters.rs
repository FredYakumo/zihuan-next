use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::node::data_value::DataType;
use crate::node::graph_io::HyperParameter;
use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{refresh_active_tab_ui, GraphTabState};
use crate::util::hyperparam_store::save_hyperparameter_values;

fn parse_hp_data_type(type_str: &str) -> DataType {
    match type_str {
        "Integer" => DataType::Integer,
        "Float" => DataType::Float,
        "Boolean" => DataType::Boolean,
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

/// Returns true if a hyperparameter with the given DataType is compatible with a port of `port_type`.
fn is_hp_type_compatible(hp_type: &DataType, port_type: &DataType) -> bool {
    match (hp_type, port_type) {
        (a, b) if a == b => true,
        // Password ports accept String hyperparameters
        (DataType::String, DataType::Password) => true,
        _ => false,
    }
}

pub(crate) fn bind_hyperparameter_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    // Open hyperparameter manager dialog
    {
        let ui_handle = ui.as_weak();
        ui.on_open_hyperparameter_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_hyperparameter_manager(true);
            }
        });
    }

    // Close hyperparameter manager dialog
    {
        let ui_handle = ui.as_weak();
        ui.on_close_hyperparameter_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_hyperparameter_manager(false);
            }
        });
    }

    // Add a new hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_add_hyperparameter(move |name, data_type_str, required, description| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            let data_type = parse_hp_data_type(data_type_str.as_str());

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                // Check unique name
                if tab.graph.hyperparameters.iter().any(|hp| hp.name == name) {
                    return;
                }
                tab.graph.hyperparameters.push(HyperParameter {
                    name,
                    data_type,
                    required,
                    description: if description.is_empty() {
                        None
                    } else {
                        Some(description.to_string())
                    },
                });
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    // Delete a hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_delete_hyperparameter(move |name| {
            let name = name.as_str();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.graph.hyperparameters.retain(|hp| hp.name != name);
                // Remove all port bindings referencing this hyperparameter
                for node in &mut tab.graph.nodes {
                    node.port_bindings.retain(|_, hp_name| hp_name != name);
                }
                tab.hyperparameter_values.remove(name);
                // Auto-save values if the graph is backed by a file
                if let Some(path) = &tab.file_path {
                    if let Err(e) = save_hyperparameter_values(path, &tab.hyperparameter_values) {
                        log::warn!("[HyperParamStore] auto-save failed: {}", e);
                    }
                }
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    // Set a hyperparameter's value
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_set_hyperparameter_value(move |name, value_str| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(hp) = tab.graph.hyperparameters.iter().find(|hp| hp.name == name.as_str()) {
                    let type_str = hp.data_type.to_string();
                    let new_value = str_to_json_value(&type_str, value_str.as_str());
                    match new_value {
                        Some(v) => { tab.hyperparameter_values.insert(hp.name.clone(), v); }
                        None => { tab.hyperparameter_values.remove(hp.name.as_str()); }
                    }
                }
                // Auto-save values if the graph is backed by a file
                if let Some(path) = &tab.file_path {
                    if let Err(e) = save_hyperparameter_values(path, &tab.hyperparameter_values) {
                        log::warn!("[HyperParamStore] auto-save failed: {}", e);
                    }
                }
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    // Toggle required flag on a hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_toggle_hyperparameter_required(move |name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(hp) = tab.graph.hyperparameters.iter_mut().find(|hp| hp.name == name.as_str()) {
                    hp.required = !hp.required;
                    tab.is_dirty = true;
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            }
        });
    }

    // Port right-clicked: populate and show bind dialog
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_port_right_clicked(move |node_id, port_name| {
            let ui = match ui_handle.upgrade() {
                Some(ui) => ui,
                None => return,
            };

            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let tab = match tabs_guard.get(active_index) {
                Some(tab) => tab,
                None => return,
            };

            // Find the port DataType
            let port_type = tab
                .graph
                .nodes
                .iter()
                .find(|n| n.id == node_id.as_str())
                .and_then(|node| {
                    node.input_ports
                        .iter()
                        .find(|p| p.name == port_name.as_str())
                        .map(|p| p.data_type.clone())
                });

            let port_type = match port_type {
                Some(t) => t,
                None => return,
            };

            // Find current binding for this port
            let current_binding = tab
                .graph
                .nodes
                .iter()
                .find(|n| n.id == node_id.as_str())
                .and_then(|n| n.port_bindings.get(port_name.as_str()))
                .cloned()
                .unwrap_or_default();

            // Filter compatible hyperparameters
            use slint::{ModelRc, VecModel};
            use crate::ui::graph_window::HyperParameterVm;
            let compatible: Vec<HyperParameterVm> = tab
                .graph
                .hyperparameters
                .iter()
                .filter(|hp| is_hp_type_compatible(&hp.data_type, &port_type))
                .map(|hp| HyperParameterVm {
                    name: hp.name.clone().into(),
                    data_type: hp.data_type.to_string().into(),
                    value: tab.hyperparameter_values
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

            ui.set_port_bind_node_id(node_id.clone());
            ui.set_port_bind_port_name(port_name.clone());
            ui.set_port_bind_current_binding(current_binding.into());
            ui.set_port_bind_compatible_params(ModelRc::new(VecModel::from(compatible)));
            ui.set_show_port_bind_dialog(true);
        });
    }

    // Bind a port to a hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_bind_port_hyperparameter(move |node_id, port_name, hp_name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                    node.port_bindings
                        .insert(port_name.to_string(), hp_name.to_string());
                    tab.is_dirty = true;
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_show_port_bind_dialog(false);
            }
        });
    }

    // Unbind a port from its hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_unbind_port_hyperparameter(move |node_id, port_name| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                    node.port_bindings.remove(port_name.as_str());
                    tab.is_dirty = true;
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_show_port_bind_dialog(false);
            }
        });
    }

    // Close the port bind dialog
    {
        let ui_handle = ui.as_weak();
        ui.on_close_port_bind_dialog(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_port_bind_dialog(false);
            }
        });
    }
}
