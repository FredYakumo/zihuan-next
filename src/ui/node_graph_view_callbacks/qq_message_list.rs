use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedString};

use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{tab_display_title, update_tabs_ui, GraphTabState};
use crate::ui::node_graph_view_inline::{get_message_list_inline, set_message_list_inline};
use crate::ui::node_graph_view_vm::apply_graph_to_ui;

pub(crate) fn bind_qq_message_list_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_qq_message_list_add(move |node_id: SharedString| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(&tab.inline_inputs, node_id.as_str());
            items.push(serde_json::json!({"type": "text", "data": {"text": ""}}));
            set_message_list_inline(&mut tab.inline_inputs, node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_qq_message_list_insert(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(&tab.inline_inputs, node_id.as_str());
            let len = items.len();
            let mut insert_at = if index < 0 {
                0
            } else {
                (index as usize).saturating_add(1)
            };
            if insert_at > len {
                insert_at = len;
            }
            items.insert(
                insert_at,
                serde_json::json!({"type": "text", "data": {"text": ""}}),
            );
            set_message_list_inline(&mut tab.inline_inputs, node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_qq_message_list_cycle_type(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(&tab.inline_inputs, node_id.as_str());
            if index >= 0 {
                let idx = index as usize;
                if let Some(serde_json::Value::Object(map)) = items.get_mut(idx) {
                    let current = map.get("type").and_then(|v| v.as_str()).unwrap_or("text");
                    let (next_type, next_data) = match current {
                        "text" => ("at", serde_json::json!({"qq": ""})),
                        "at" => ("reply", serde_json::json!({"id": 0})),
                        _ => ("text", serde_json::json!({"text": ""})),
                    };
                    map.insert(
                        "type".to_string(),
                        serde_json::Value::String(next_type.to_string()),
                    );
                    map.insert("data".to_string(), next_data);
                }
            }
            set_message_list_inline(&mut tab.inline_inputs, node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    &tab.graph,
                    Some(tab_display_title(tab)),
                    &tab.selection,
                    &tab.inline_inputs,
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_qq_message_list_set_content(
        move |node_id: SharedString, index: i32, value: SharedString| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let mut items = get_message_list_inline(&tab.inline_inputs, node_id.as_str());
                if index >= 0 {
                    let idx = index as usize;
                    if let Some(serde_json::Value::Object(map)) = items.get_mut(idx) {
                        let msg_type = map
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text")
                            .to_string();
                        let data_obj = map.entry("data").or_insert_with(|| serde_json::json!({}));
                        if let serde_json::Value::Object(data) = data_obj {
                            match msg_type.as_str() {
                                "text" => {
                                    data.insert(
                                        "text".to_string(),
                                        serde_json::Value::String(value.to_string()),
                                    );
                                }
                                "at" => {
                                    data.remove("target");
                                    data.insert(
                                        "qq".to_string(),
                                        serde_json::Value::String(value.to_string()),
                                    );
                                }
                                "reply" => {
                                    let id: i64 = value.as_str().parse().unwrap_or(0);
                                    data.insert("id".to_string(), serde_json::json!(id));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                set_message_list_inline(&mut tab.inline_inputs, node_id.as_str(), items);
                tab.is_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    update_tabs_ui(&ui, &tabs_guard, active_index);
                }
            }
        },
    );
}
