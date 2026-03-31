use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, SharedString};

use crate::ui::graph_window::NodeGraphWindow;
use crate::ui::node_graph_view::{update_tabs_ui, GraphTabState};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

pub(crate) fn bind_inline_port_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_inline_port_text_changed(
        move |node_id: SharedString, port_name: SharedString, value: SharedString| {
            let key = inline_port_key(node_id.as_str(), port_name.as_str());
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.inline_inputs_mut()
                    .insert(key, InlinePortValue::Text(value.to_string()));
                tab.is_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    update_tabs_ui(&ui, &tabs_guard, active_index);
                }
            }
        },
    );

    let tabs_clone = Arc::clone(&tabs);
    let active_tab_clone = Arc::clone(&active_tab_index);
    let ui_handle = ui.as_weak();
    ui.on_inline_port_bool_changed(
        move |node_id: SharedString, port_name: SharedString, value: bool| {
            let key = inline_port_key(node_id.as_str(), port_name.as_str());
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                tab.inline_inputs_mut().insert(key, InlinePortValue::Bool(value));
                tab.is_dirty = true;
                if let Some(ui) = ui_handle.upgrade() {
                    update_tabs_ui(&ui, &tabs_guard, active_index);
                }
            }
        },
    );
}
