use std::sync::{Arc, Mutex};

use slint::ComponentHandle;

use crate::node::data_value::DataType;
use crate::node::graph_io::HyperParameter;
use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{
    collect_hyperparameter_groups, refresh_active_tab_ui, update_tabs_ui, GraphTabState,
};
use crate::util::hyperparam_store::save_hyperparameter_values;

fn parse_hp_data_type(type_str: &str) -> DataType {
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

/// Returns true if a hyperparameter with the given DataType is compatible with a port of `port_type`.
fn is_hp_type_compatible(hp_type: &DataType, port_type: &DataType) -> bool {
    match (hp_type, port_type) {
        (a, b) if a == b => true,
        // Password ports accept String hyperparameters
        (DataType::String, DataType::Password) => true,
        // Password hyperparameters can also bind to String ports
        (DataType::Password, DataType::String) => true,
        _ => false,
    }
}

fn normalize_group_name(group: &str) -> String {
    let trimmed = group.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
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

    {
        let ui_handle = ui.as_weak();
        ui.on_open_group_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_group_manager(true);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_group_manager(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_group_manager(false);
            }
        });
    }

    // Add a new hyperparameter
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_add_hyperparameter(move |name, data_type_str, required, description, group| {
            let name = name.trim().to_string();
            if name.is_empty() {
                return;
            }
            let data_type = parse_hp_data_type(data_type_str.as_str());
            let group = normalize_group_name(group.as_str());

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                // Check unique name
                if tab.graph.hyperparameters.iter().any(|hp| hp.name == name) {
                    return;
                }
                if !tab.graph.hyperparameter_groups.iter().any(|g| g == &group) {
                    tab.graph.hyperparameter_groups.push(group.clone());
                }
                tab.graph.hyperparameters.push(HyperParameter {
                    name,
                    data_type,
                    group,
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

    {
        let ui_handle = ui.as_weak();
        ui.on_select_hyperparameter_group(move |group| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_selected_hyperparameter_group(normalize_group_name(group.as_str()).into());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_create_hyperparameter_group(move |group| {
            let group = normalize_group_name(group.as_str());
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if !tab
                    .graph
                    .hyperparameter_groups
                    .iter()
                    .any(|existing| existing == &group)
                {
                    tab.graph.hyperparameter_groups.push(group.clone());
                    tab.is_dirty = true;
                }
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_selected_hyperparameter_group(group.into());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_rename_hyperparameter_group(move |old_group, new_group| {
            let old_group = normalize_group_name(old_group.as_str());
            let new_group = normalize_group_name(new_group.as_str());
            if old_group == "default" || new_group == "default" || old_group == new_group {
                return;
            }

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if tab
                    .graph
                    .hyperparameter_groups
                    .iter()
                    .any(|existing| existing == &new_group)
                {
                    return;
                }
                if let Some(group) = tab
                    .graph
                    .hyperparameter_groups
                    .iter_mut()
                    .find(|existing| **existing == old_group)
                {
                    *group = new_group.clone();
                }
                for hp in &mut tab.graph.hyperparameters {
                    if hp.group == old_group {
                        hp.group = new_group.clone();
                    }
                }
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                if ui.get_selected_hyperparameter_group().as_str() == old_group.as_str() {
                    ui.set_selected_hyperparameter_group(new_group.into());
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_delete_hyperparameter_group(move |group| {
            let group = normalize_group_name(group.as_str());
            if group == "default" {
                return;
            }

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.graph
                    .hyperparameter_groups
                    .retain(|existing| existing != &group);
                for hp in &mut tab.graph.hyperparameters {
                    if hp.group == group {
                        hp.group = "default".to_string();
                    }
                }
                if !tab
                    .graph
                    .hyperparameter_groups
                    .iter()
                    .any(|existing| existing == "default")
                {
                    tab.graph.hyperparameter_groups.push("default".to_string());
                }
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                refresh_active_tab_ui(&ui, &tabs_guard, active_index);
                ui.set_selected_hyperparameter_group("default".into());
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
                    if let Err(e) =
                        save_hyperparameter_values(path, &tab.graph, &tab.hyperparameter_values)
                    {
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
                if let Some(hp) = tab
                    .graph
                    .hyperparameters
                    .iter()
                    .find(|hp| hp.name == name.as_str())
                {
                    let type_str = hp.data_type.to_string();
                    let new_value = str_to_json_value(&type_str, value_str.as_str());
                    match new_value {
                        Some(v) => {
                            tab.hyperparameter_values.insert(hp.name.clone(), v);
                        }
                        None => {
                            tab.hyperparameter_values.remove(hp.name.as_str());
                        }
                    }
                }
                // Auto-save values if the graph is backed by a file
                if let Some(path) = &tab.file_path {
                    if let Err(e) =
                        save_hyperparameter_values(path, &tab.graph, &tab.hyperparameter_values)
                    {
                        log::warn!("[HyperParamStore] auto-save failed: {}", e);
                    }
                }
                tab.is_dirty = true;
            }
            if let Some(ui) = ui_handle.upgrade() {
                // Targeted in-place update: only update the specific row's value.
                // A full refresh via refresh_active_tab_ui would replace the entire
                // model with a new VecModel, causing Slint to recreate all list items
                // and making the focused LineEdit lose focus on every keystroke.
                use slint::Model;
                let model = ui.get_hyperparameters();
                for i in 0..model.row_count() {
                    if let Some(mut item) = model.row_data(i) {
                        if item.name.as_str() == name.as_str() {
                            item.value = value_str.as_str().into();
                            model.set_row_data(i, item);
                            break;
                        }
                    }
                }
                // Update tab titles to reflect dirty state
                update_tabs_ui(&ui, &tabs_guard, active_index);
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
                if let Some(hp) = tab
                    .graph
                    .hyperparameters
                    .iter_mut()
                    .find(|hp| hp.name == name.as_str())
                {
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
            use crate::ui::graph_window::HyperParameterVm;
            use slint::{ModelRc, VecModel};
            let compatible: Vec<HyperParameterVm> = tab
                .graph
                .hyperparameters
                .iter()
                .filter(|hp| is_hp_type_compatible(&hp.data_type, &port_type))
                .map(|hp| HyperParameterVm {
                    name: hp.name.clone().into(),
                    group: hp.group.clone().into(),
                    data_type: hp.data_type.to_string().into(),
                    value: tab
                        .hyperparameter_values
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
            let groups = collect_hyperparameter_groups(&tab.graph);
            if !groups
                .iter()
                .any(|group| group == &ui.get_selected_hyperparameter_group().to_string())
            {
                ui.set_selected_hyperparameter_group("default".into());
            }
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
                if let Some(node) = tab
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == node_id.as_str())
                {
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
                if let Some(node) = tab
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == node_id.as_str())
                {
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
