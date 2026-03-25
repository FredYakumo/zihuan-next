use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::node::{DataType, Port};
use crate::node::util::format_string::find_incomplete_variable;
use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{GraphTabState, refresh_active_tab_ui};
use crate::ui::node_render::{InlinePortValue, inline_port_key};

const TEMPLATE_PORT: &str = "template";

fn extract_variables(template: &str) -> Vec<String> {
    let mut vars = vec![];
    let mut seen = HashSet::new();
    let mut pos = 0;
    while let Some(rel) = template[pos..].find("${") {
        let start = pos + rel + 2;
        if let Some(end_rel) = template[start..].find('}') {
            let name = template[start..start + end_rel].trim().to_string();
            if !name.is_empty() && seen.insert(name.clone()) {
                vars.push(name);
            }
            pos = start + end_rel + 1;
        } else {
            break;
        }
    }
    vars
}

fn format_string_input_ports(variables: &[String]) -> Vec<Port> {
    variables
        .iter()
        .map(|var| {
            Port::new(var.clone(), DataType::String)
                .with_description(format!("变量 {var}"))
        })
        .collect()
}

fn set_variables(ui: &NodeGraphWindow, vars: &[String]) {
    let items: Vec<SharedString> = vars.iter().map(|v| v.as_str().into()).collect();
    ui.set_format_string_editor_variables(ModelRc::new(VecModel::from(items)));
}

fn set_suggestions(ui: &NodeGraphWindow, suggestions: &[String]) {
    let items: Vec<SharedString> = suggestions.iter().map(|v| v.as_str().into()).collect();
    ui.set_format_string_editor_suggestions(ModelRc::new(VecModel::from(items)));
}

fn update_variables_and_autocomplete(ui: &NodeGraphWindow, text: &str) {
    let vars = extract_variables(text);
    set_variables(ui, &vars);

    if let Some(prefix) = find_incomplete_variable(text) {
        let prefix_lower = prefix.to_lowercase();
        let suggestions: Vec<String> = vars
            .iter()
            .filter(|v| v.to_lowercase().starts_with(&prefix_lower))
            .cloned()
            .collect();
        if !suggestions.is_empty() {
            set_suggestions(ui, &suggestions);
            ui.set_format_string_show_autocomplete(true);
            return;
        }
    }

    set_suggestions(ui, &[]);
    ui.set_format_string_show_autocomplete(false);
}

pub(crate) fn bind_format_string_editor_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    // edit_format_string_clicked → open_format_string_editor
    {
        let ui_handle = ui.as_weak();
        ui.on_edit_format_string_clicked(move |node_id| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.invoke_open_format_string_editor(node_id);
            }
        });
    }

    // open_format_string_editor: load template, populate dialog
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_open_format_string_editor(move |node_id| {
            let Some(ui) = ui_handle.upgrade() else { return };

            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let Some(tab) = tabs_guard.get(active_index) else { return };
            let Some(node) = tab.graph.nodes.iter().find(|n| n.id == node_id.as_str()) else {
                return;
            };

            let template = node
                .inline_values
                .get(TEMPLATE_PORT)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let vars = extract_variables(&template);
            ui.set_format_string_editor_node_id(node_id);
            ui.set_format_string_editor_template(template.as_str().into());
            set_variables(&ui, &vars);
            set_suggestions(&ui, &[]);
            ui.set_format_string_show_autocomplete(false);
            ui.set_show_format_string_editor(true);
        });
    }

    // close_format_string_editor: hide and clear dialog state
    {
        let ui_handle = ui.as_weak();
        ui.on_close_format_string_editor(move || {
            let Some(ui) = ui_handle.upgrade() else { return };
            ui.set_show_format_string_editor(false);
            ui.set_format_string_editor_node_id("".into());
            ui.set_format_string_editor_template("".into());
            set_variables(&ui, &[]);
            set_suggestions(&ui, &[]);
            ui.set_format_string_show_autocomplete(false);
        });
    }

    // format_string_text_changed: parse variables + autocomplete
    {
        let ui_handle = ui.as_weak();
        ui.on_format_string_text_changed(move |text| {
            let Some(ui) = ui_handle.upgrade() else { return };
            update_variables_and_autocomplete(&ui, text.as_str());
        });
    }

    // format_string_autocomplete_select: complete the last incomplete variable
    {
        let ui_handle = ui.as_weak();
        ui.on_format_string_autocomplete_select(move |suggestion| {
            let Some(ui) = ui_handle.upgrade() else { return };
            let current = ui.get_format_string_editor_template();
            let text = current.as_str();

            let new_text = if let Some(last_open) = text.rfind("${") {
                let after = &text[last_open + 2..];
                if !after.contains('}') {
                    format!("{}${{{}}}", &text[..last_open], suggestion.as_str())
                } else {
                    text.to_string()
                }
            } else {
                text.to_string()
            };

            ui.set_format_string_editor_template(new_text.as_str().into());
            update_variables_and_autocomplete(&ui, &new_text);
        });
    }

    // save_format_string_editor: update node ports and inline values
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_save_format_string_editor(move || {
            let Some(ui) = ui_handle.upgrade() else { return };

            let template = ui.get_format_string_editor_template().to_string();
            let node_id = ui.get_format_string_editor_node_id().to_string();
            let variables = extract_variables(&template);
            let port_names: HashSet<String> = variables.iter().cloned().collect();
            let new_ports = format_string_input_ports(&variables);

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id) {
                    node.inline_values
                        .insert(TEMPLATE_PORT.to_string(), serde_json::Value::String(template.clone()));
                    node.input_ports = new_ports;

                    // Remove edges whose target port no longer exists on this node
                    tab.graph.edges.retain(|edge| {
                        if edge.to_node_id == node.id {
                            port_names.contains(&edge.to_port)
                        } else {
                            true
                        }
                    });

                    tab.inline_inputs.insert(
                        inline_port_key(&node.id, TEMPLATE_PORT),
                        InlinePortValue::Text(template),
                    );
                    tab.is_dirty = true;
                }
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_format_string_editor(false);
        });
    }
}

