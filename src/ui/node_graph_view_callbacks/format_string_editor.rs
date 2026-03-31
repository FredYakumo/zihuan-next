use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::node::util::format_string::{
    complete_incomplete_variable_at, find_incomplete_variable_at,
};
use crate::node::{DataType, Port};
use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{refresh_active_tab_ui, GraphTabState};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

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

fn extract_existing_variables_from_ports(input_ports: &[Port]) -> Vec<String> {
    let mut vars = Vec::new();
    let mut seen = HashSet::new();
    for port in input_ports {
        let name = port.name.trim();
        if name.is_empty() || name == TEMPLATE_PORT {
            continue;
        }
        if seen.insert(name.to_string()) {
            vars.push(name.to_string());
        }
    }
    vars
}

fn merge_suggestion_pool(existing_vars: &[String], template_vars: &[String]) -> Vec<String> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();

    for name in existing_vars.iter().chain(template_vars.iter()) {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_string()) {
            merged.push(trimmed.to_string());
        }
    }

    merged
}

fn format_string_input_ports(variables: &[String]) -> Vec<Port> {
    variables
        .iter()
        .map(|var| Port::new(var.clone(), DataType::String).with_description(format!("变量 {var}")))
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

fn clear_autocomplete(ui: &NodeGraphWindow, suggestions_state: &Arc<Mutex<Vec<String>>>) {
    *suggestions_state.lock().unwrap() = Vec::new();
    set_suggestions(ui, &[]);
    ui.set_format_string_show_autocomplete(false);
    ui.set_format_string_autocomplete_selected_index(-1);
}

fn update_variables_and_autocomplete(
    ui: &NodeGraphWindow,
    text: &str,
    cursor_offset: usize,
    existing_vars: &[String],
    suggestions_state: &Arc<Mutex<Vec<String>>>,
) {
    let vars = extract_variables(text);
    set_variables(ui, &vars);

    if let Some(ctx) = find_incomplete_variable_at(text, cursor_offset) {
        let prefix_lower = ctx.prefix.to_lowercase();
        let suggestion_pool = merge_suggestion_pool(existing_vars, &vars);
        let suggestions: Vec<String> = suggestion_pool
            .into_iter()
            .filter(|v| v.to_lowercase().starts_with(&prefix_lower))
            .collect();

        if !suggestions.is_empty() {
            *suggestions_state.lock().unwrap() = suggestions.clone();
            set_suggestions(ui, &suggestions);
            ui.set_format_string_show_autocomplete(true);
            ui.set_format_string_autocomplete_selected_index(0);
            return;
        }
    }

    clear_autocomplete(ui, suggestions_state);
}

fn apply_suggestion(
    ui: &NodeGraphWindow,
    suggestion: &str,
    cursor_offset: usize,
    existing_vars: &[String],
    cursor_offset_state: &Arc<Mutex<usize>>,
    suggestions_state: &Arc<Mutex<Vec<String>>>,
) {
    let current = ui.get_format_string_editor_template();
    let current_text = current.as_str();

    let new_cursor = find_incomplete_variable_at(current_text, cursor_offset)
        .map(|ctx| ctx.open_index + suggestion.len() + 3)
        .unwrap_or(cursor_offset);

    let Some(new_text) = complete_incomplete_variable_at(current_text, cursor_offset, suggestion)
    else {
        return;
    };

    ui.set_format_string_editor_template(new_text.as_str().into());
    *cursor_offset_state.lock().unwrap() = new_cursor;
    update_variables_and_autocomplete(ui, &new_text, new_cursor, existing_vars, suggestions_state);
}

pub(crate) fn bind_format_string_editor_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    let existing_vars_state: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cursor_offset_state: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let suggestions_state: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

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
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_open_format_string_editor(move |node_id| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let Some(tab) = tabs_guard.get(active_index) else {
                return;
            };
            let Some(node) = tab.graph().nodes.iter().find(|n| n.id == node_id.as_str()) else {
                return;
            };

            let template = node
                .inline_values
                .get(TEMPLATE_PORT)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let existing_vars = extract_existing_variables_from_ports(&node.input_ports);
            let cursor = template.len();

            *existing_vars_clone.lock().unwrap() = existing_vars.clone();
            *cursor_offset_clone.lock().unwrap() = cursor;

            ui.set_format_string_editor_node_id(node_id);
            ui.set_format_string_editor_template(template.as_str().into());
            update_variables_and_autocomplete(
                &ui,
                &template,
                cursor,
                &existing_vars,
                &suggestions_clone,
            );
            ui.set_show_format_string_editor(true);
        });
    }

    // close_format_string_editor: hide and clear dialog state
    {
        let ui_handle = ui.as_weak();
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_close_format_string_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            ui.set_show_format_string_editor(false);
            ui.set_format_string_editor_node_id("".into());
            ui.set_format_string_editor_template("".into());
            set_variables(&ui, &[]);
            clear_autocomplete(&ui, &suggestions_clone);
            *existing_vars_clone.lock().unwrap() = Vec::new();
            *cursor_offset_clone.lock().unwrap() = 0;
        });
    }

    // format_string_text_changed: parse variables + autocomplete
    {
        let ui_handle = ui.as_weak();
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_format_string_text_changed(move |text, cursor_offset| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let cursor = (cursor_offset as i64).max(0) as usize;
            *cursor_offset_clone.lock().unwrap() = cursor;
            let existing_vars = existing_vars_clone.lock().unwrap().clone();
            update_variables_and_autocomplete(
                &ui,
                text.as_str(),
                cursor,
                &existing_vars,
                &suggestions_clone,
            );
        });
    }

    // format_string_autocomplete_select: complete variable by click
    {
        let ui_handle = ui.as_weak();
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_format_string_autocomplete_select(move |suggestion| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let cursor = *cursor_offset_clone.lock().unwrap();
            let existing_vars = existing_vars_clone.lock().unwrap().clone();
            apply_suggestion(
                &ui,
                suggestion.as_str(),
                cursor,
                &existing_vars,
                &cursor_offset_clone,
                &suggestions_clone,
            );
        });
    }

    // format_string_autocomplete_navigate: move selected suggestion
    {
        let ui_handle = ui.as_weak();
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_format_string_autocomplete_navigate(move |delta| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let suggestions = suggestions_clone.lock().unwrap().clone();
            if suggestions.is_empty() {
                return;
            }

            let len = suggestions.len() as i32;
            let current = ui.get_format_string_autocomplete_selected_index();
            let base = if current >= 0 && current < len {
                current
            } else {
                0
            };
            let next = (base + delta).rem_euclid(len);
            ui.set_format_string_autocomplete_selected_index(next);
        });
    }

    // format_string_autocomplete_accept: complete selected suggestion by keyboard
    {
        let ui_handle = ui.as_weak();
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_format_string_autocomplete_accept(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let suggestions = suggestions_clone.lock().unwrap().clone();
            if suggestions.is_empty() {
                return;
            }

            let len = suggestions.len() as i32;
            let selected = ui.get_format_string_autocomplete_selected_index();
            let index = if selected >= 0 && selected < len {
                selected as usize
            } else {
                0
            };
            let suggestion = suggestions[index].clone();
            let cursor = *cursor_offset_clone.lock().unwrap();
            let existing_vars = existing_vars_clone.lock().unwrap().clone();
            apply_suggestion(
                &ui,
                &suggestion,
                cursor,
                &existing_vars,
                &cursor_offset_clone,
                &suggestions_clone,
            );
        });
    }

    // save_format_string_editor: update node ports and inline values
    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        let existing_vars_clone = Arc::clone(&existing_vars_state);
        let cursor_offset_clone = Arc::clone(&cursor_offset_state);
        let suggestions_clone = Arc::clone(&suggestions_state);
        ui.on_save_format_string_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let template = ui.get_format_string_editor_template().to_string();
            let node_id = ui.get_format_string_editor_node_id().to_string();
            let variables = extract_variables(&template);
            let port_names: HashSet<String> = variables.iter().cloned().collect();
            let new_ports = format_string_input_ports(&variables);

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node_index) = tab.graph.nodes.iter().position(|n| n.id == node_id) {
                    let node_id_for_ports = tab.graph.nodes[node_index].id.clone();
                    let node = &mut tab.graph.nodes[node_index];
                    node.inline_values.insert(
                        TEMPLATE_PORT.to_string(),
                        serde_json::Value::String(template.clone()),
                    );
                    node.input_ports = new_ports;

                    // Remove edges whose target port no longer exists on this node
                    tab.graph.edges.retain(|edge| {
                        if edge.to_node_id == node_id_for_ports {
                            port_names.contains(&edge.to_port)
                        } else {
                            true
                        }
                    });

                    tab.inline_inputs.insert(
                        inline_port_key(&node_id_for_ports, TEMPLATE_PORT),
                        InlinePortValue::Text(template),
                    );
                    tab.is_dirty = true;
                }
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_format_string_editor(false);
            clear_autocomplete(&ui, &suggestions_clone);
            *existing_vars_clone.lock().unwrap() = Vec::new();
            *cursor_offset_clone.lock().unwrap() = 0;
        });
    }
}
