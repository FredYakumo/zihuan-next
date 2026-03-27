use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::node::DataType;
use crate::node::Port;
use crate::ui::graph_window::{JsonExtractFieldVm, NodeGraphWindow};
use crate::ui::node_graph_view::{refresh_active_tab_ui, GraphTabState};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

const FIELDS_CONFIG_PORT: &str = "fields_config";

fn default_field_vm() -> JsonExtractFieldVm {
    JsonExtractFieldVm {
        name: "field".into(),
        data_type: "String".into(),
    }
}

fn field_items_from_json(value: &serde_json::Value) -> Vec<JsonExtractFieldVm> {
    value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|field| field.as_object().cloned())
        .map(|field| JsonExtractFieldVm {
            name: field
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            data_type: field
                .get("data_type")
                .and_then(|v| v.as_str())
                .unwrap_or("String")
                .into(),
        })
        .collect()
}

fn field_items_to_json(items: &[JsonExtractFieldVm]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|field| {
                serde_json::json!({
                    "name": field.name.as_str(),
                    "data_type": field.data_type.as_str(),
                })
            })
            .collect(),
    )
}

fn read_field_items(ui: &NodeGraphWindow) -> Vec<JsonExtractFieldVm> {
    ui.get_json_extract_editor_items().iter().collect()
}

fn write_field_items(ui: &NodeGraphWindow, items: Vec<JsonExtractFieldVm>) {
    ui.set_json_extract_editor_items(ModelRc::new(VecModel::from(items)));
}

fn replace_field_row(ui: &NodeGraphWindow, index: usize, new_item: JsonExtractFieldVm) {
    let model = ui.get_json_extract_editor_items();
    if index < model.row_count() {
        model.set_row_data(index, new_item);
    }
}

fn string_to_data_type(value: &str) -> DataType {
    match value {
        "String" => DataType::String,
        "Integer" => DataType::Integer,
        "Float" => DataType::Float,
        "Boolean" => DataType::Boolean,
        "Json" => DataType::Json,
        _ => DataType::String,
    }
}

fn json_extract_output_ports(items: &[JsonExtractFieldVm]) -> Vec<Port> {
    items
        .iter()
        .map(|field| {
            Port::new(
                field.name.as_str(),
                string_to_data_type(field.data_type.as_str()),
            )
            .with_description(format!("从输入 JSON 中提取字段 '{}'", field.name))
        })
        .collect()
}

fn validate_fields(items: &[JsonExtractFieldVm]) -> Result<(), String> {
    let mut field_names = HashSet::new();

    for field in items {
        let field_name = field.name.as_str().trim();
        if field_name.is_empty() {
            return Err("提取字段名不能为空".to_string());
        }
        if !field_names.insert(field_name.to_string()) {
            return Err(format!("提取字段名重复：{}", field_name));
        }
    }

    Ok(())
}

pub(crate) fn bind_json_extract_editor_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    {
        let ui_handle = ui.as_weak();
        ui.on_edit_json_extract_clicked(move |node_id| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.invoke_open_json_extract_editor(node_id);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_open_json_extract_editor(move |node_id| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let Some(tab) = tabs_guard.get(active_index) else {
                return;
            };
            let Some(node) = tab.graph.nodes.iter().find(|n| n.id == node_id.as_str()) else {
                return;
            };

            let items = node
                .inline_values
                .get(FIELDS_CONFIG_PORT)
                .map(field_items_from_json)
                .unwrap_or_default();

            ui.set_json_extract_editor_node_id(node_id);
            ui.set_json_extract_editor_selected_index(if items.is_empty() { -1 } else { 0 });
            write_field_items(&ui, items);
            ui.set_show_json_extract_editor_dialog(true);
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_json_extract_editor(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_json_extract_editor_dialog(false);
                ui.set_json_extract_editor_node_id("".into());
                ui.set_json_extract_editor_selected_index(-1);
                write_field_items(&ui, Vec::new());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_json_extract_editor_select(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_json_extract_editor_selected_index(index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_json_extract_editor_add_field(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut items = read_field_items(&ui);
                items.push(default_field_vm());
                let next_index = items.len().saturating_sub(1) as i32;
                write_field_items(&ui, items);
                ui.set_json_extract_editor_selected_index(next_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_json_extract_editor_delete_field(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                let mut items = read_field_items(&ui);
                let idx = index.max(0) as usize;
                if idx < items.len() {
                    items.remove(idx);
                }
                let next_index = if items.is_empty() {
                    -1
                } else if idx >= items.len() {
                    (items.len() - 1) as i32
                } else {
                    idx as i32
                };
                write_field_items(&ui, items);
                ui.set_json_extract_editor_selected_index(next_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_json_extract_editor_set_field_name(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_json_extract_editor_items();
                if let Some(mut item) = model.row_data(idx) {
                    item.name = value;
                    replace_field_row(&ui, idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_json_extract_editor_set_field_type(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_json_extract_editor_items();
                if let Some(mut item) = model.row_data(idx) {
                    item.data_type = value;
                    replace_field_row(&ui, idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_save_json_extract_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let items = read_field_items(&ui);
            if let Err(message) = validate_fields(&items) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }

            let fields_json = field_items_to_json(&items);
            let node_id = ui.get_json_extract_editor_node_id();

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                    node.inline_values
                        .insert(FIELDS_CONFIG_PORT.to_string(), fields_json.clone());
                    node.output_ports = json_extract_output_ports(&items);
                    let output_names: HashSet<&str> =
                        node.output_ports.iter().map(|p| p.name.as_str()).collect();
                    tab.graph.edges.retain(|edge| {
                        if edge.from_node_id == node.id {
                            output_names.contains(edge.from_port.as_str())
                        } else {
                            true
                        }
                    });
                    tab.inline_inputs.insert(
                        inline_port_key(&node.id, FIELDS_CONFIG_PORT),
                        InlinePortValue::Json(fields_json),
                    );
                    tab.is_dirty = true;
                }
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_json_extract_editor_dialog(false);
        });
    }
}
