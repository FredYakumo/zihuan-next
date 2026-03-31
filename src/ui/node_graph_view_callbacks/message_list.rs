use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedString};

use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{tab_display_title, update_tabs_ui, GraphTabState};
use crate::ui::node_graph_view_inline::{
    cycle_role, get_message_list_inline, new_message_item, set_message_list_inline,
};
use crate::ui::node_graph_view_vm::apply_graph_to_ui;

pub(crate) fn bind_message_list_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_add(move |node_id: SharedString| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            items.push(new_message_item("user", ""));
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_insert(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            let len = items.len();
            let mut insert_at = if index < 0 {
                0
            } else {
                (index as usize).saturating_add(1)
            };
            if insert_at > len {
                insert_at = len;
            }
            items.insert(insert_at, new_message_item("user", ""));
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_delete(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            if index >= 0 {
                let idx = index as usize;
                if idx < items.len() {
                    items.remove(idx);
                }
            }
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_move_up(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            if index > 0 {
                let idx = index as usize;
                if idx < items.len() {
                    items.swap(idx - 1, idx);
                }
            }
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_move_down(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            if index >= 0 {
                let idx = index as usize;
                if idx + 1 < items.len() {
                    items.swap(idx, idx + 1);
                }
            }
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_cycle_role(move |node_id: SharedString, index: i32| {
        let mut tabs_guard = tabs_clone.lock().unwrap();
        let active_index = *active_tab_clone.lock().unwrap();
        if let Some(tab) = tabs_guard.get_mut(active_index) {
            let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
            if index >= 0 {
                let idx = index as usize;
                if let Some(serde_json::Value::Object(map)) = items.get_mut(idx) {
                    let current = map.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                    map.insert(
                        "role".to_string(),
                        serde_json::Value::String(cycle_role(current).to_string()),
                    );
                }
            }
            set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
            tab.is_dirty = true;
            if let Some(ui) = ui_handle.upgrade() {
                apply_graph_to_ui(
                    &ui,
                    tab.graph(),
                    tab.root_graph().variables.as_slice(),
                    Some(tab_display_title(tab)),
                    tab.selection(),
                    tab.inline_inputs(),
                    &tab.hyperparameter_values,
                );
                update_tabs_ui(&ui, &tabs_guard, active_index);
            }
        }
    });

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_message_list_set_content(
        move |node_id: SharedString, index: i32, value: SharedString| {
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let mut items = get_message_list_inline(tab.inline_inputs(), node_id.as_str());
                if index >= 0 {
                    let idx = index as usize;
                    if let Some(serde_json::Value::Object(map)) = items.get_mut(idx) {
                        map.insert(
                            "content".to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }
                set_message_list_inline(tab.inline_inputs_mut(), node_id.as_str(), items);
                tab.is_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    update_tabs_ui(&ui, &tabs_guard, active_index);
                }
            }
        },
    );
}
