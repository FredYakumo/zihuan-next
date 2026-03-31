use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::node::function_graph::{
    default_embedded_function_config, embedded_function_config_from_node,
    sync_function_node_definition, FunctionPortDef, FUNCTION_CONFIG_PORT,
};
use crate::node::DataType;
use crate::ui::graph_window::{FunctionPortVm, NodeGraphWindow};
use crate::ui::node_graph_view::{
    apply_canvas_view_state, refresh_active_tab_ui, GraphTabState, SubgraphOwner,
};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

fn default_port_vm(prefix: &str) -> FunctionPortVm {
    FunctionPortVm {
        name: prefix.into(),
        data_type: "String".into(),
    }
}

fn string_to_data_type(value: &str) -> DataType {
    match value {
        "String" => DataType::String,
        "Integer" => DataType::Integer,
        "Float" => DataType::Float,
        "Boolean" => DataType::Boolean,
        "Json" => DataType::Json,
        "OpenAIMessage" => DataType::OpenAIMessage,
        "QQMessage" => DataType::QQMessage,
        "MessageEvent" => DataType::MessageEvent,
        _ => DataType::String,
    }
}

fn read_ports(model: ModelRc<FunctionPortVm>) -> Vec<FunctionPortVm> {
    model.iter().collect()
}

fn write_ports(ui: &NodeGraphWindow, inputs: Vec<FunctionPortVm>, outputs: Vec<FunctionPortVm>) {
    ui.set_function_editor_inputs(ModelRc::new(VecModel::from(inputs)));
    ui.set_function_editor_outputs(ModelRc::new(VecModel::from(outputs)));
}

fn validate_ports(label: &str, ports: &[FunctionPortVm]) -> Result<(), String> {
    let mut names = HashSet::new();
    for port in ports {
        let name = port.name.as_str().trim();
        if name.is_empty() {
            return Err(format!("{}名称不能为空", label));
        }
        if !names.insert(name.to_string()) {
            return Err(format!("{}名称重复：{}", label, name));
        }
    }
    Ok(())
}

fn ports_from_vm(ports: &[FunctionPortVm]) -> Vec<FunctionPortDef> {
    ports
        .iter()
        .map(|port| FunctionPortDef {
            name: port.name.to_string(),
            data_type: string_to_data_type(port.data_type.as_str()),
        })
        .collect()
}

fn load_function_editor(ui: &NodeGraphWindow, tab: &GraphTabState, node_id: &str) {
    let Some(node) = tab.graph.nodes.iter().find(|node| node.id == node_id) else {
        return;
    };
    let config = embedded_function_config_from_node(node)
        .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));

    ui.set_function_editor_node_id(node_id.into());
    ui.set_function_editor_name(config.name.into());
    ui.set_function_editor_description(config.description.into());
    write_ports(
        ui,
        config
            .inputs
            .into_iter()
            .map(|port| FunctionPortVm {
                name: port.name.into(),
                data_type: port.data_type.to_string().into(),
            })
            .collect(),
        config
            .outputs
            .into_iter()
            .map(|port| FunctionPortVm {
                name: port.name.into(),
                data_type: port.data_type.to_string().into(),
            })
            .collect(),
    );
}

pub(crate) fn bind_function_editor_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    {
        let ui_handle = ui.as_weak();
        ui.on_edit_function_clicked(move |node_id| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.invoke_open_function_editor(node_id);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_open_function_editor(move |node_id| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let Some(tab) = tabs_guard.get(active_index) else {
                return;
            };
            load_function_editor(&ui, tab, node_id.as_str());
            ui.set_show_function_editor_dialog(true);
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_function_editor(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_function_editor_dialog(false);
                ui.set_function_editor_node_id("".into());
                ui.set_function_editor_name("".into());
                ui.set_function_editor_description("".into());
                write_ports(&ui, Vec::new(), Vec::new());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_name(move |value| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_function_editor_name(value);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_description(move |value| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_function_editor_description(value);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_add_input(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut inputs = read_ports(ui.get_function_editor_inputs());
                inputs.push(default_port_vm("input"));
                let outputs = read_ports(ui.get_function_editor_outputs());
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_delete_input(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let mut inputs = read_ports(ui.get_function_editor_inputs());
                if idx < inputs.len() {
                    inputs.remove(idx);
                }
                let outputs = read_ports(ui.get_function_editor_outputs());
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_input_name(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let mut inputs = read_ports(ui.get_function_editor_inputs());
                if let Some(port) = inputs.get_mut(idx) {
                    port.name = value;
                }
                let outputs = read_ports(ui.get_function_editor_outputs());
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_input_type(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let mut inputs = read_ports(ui.get_function_editor_inputs());
                if let Some(port) = inputs.get_mut(idx) {
                    port.data_type = value;
                }
                let outputs = read_ports(ui.get_function_editor_outputs());
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_add_output(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let inputs = read_ports(ui.get_function_editor_inputs());
                let mut outputs = read_ports(ui.get_function_editor_outputs());
                outputs.push(default_port_vm("output"));
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_delete_output(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let inputs = read_ports(ui.get_function_editor_inputs());
                let mut outputs = read_ports(ui.get_function_editor_outputs());
                if idx < outputs.len() {
                    outputs.remove(idx);
                }
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_output_name(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let inputs = read_ports(ui.get_function_editor_inputs());
                let mut outputs = read_ports(ui.get_function_editor_outputs());
                if let Some(port) = outputs.get_mut(idx) {
                    port.name = value;
                }
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_function_editor_set_output_type(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let inputs = read_ports(ui.get_function_editor_inputs());
                let mut outputs = read_ports(ui.get_function_editor_outputs());
                if let Some(port) = outputs.get_mut(idx) {
                    port.data_type = value;
                }
                write_ports(&ui, inputs, outputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_save_function_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let name = ui.get_function_editor_name().to_string();
            if name.trim().is_empty() {
                ui.set_error_dialog_message("函数名称不能为空".into());
                ui.set_show_error_dialog(true);
                return;
            }
            let inputs = read_ports(ui.get_function_editor_inputs());
            let outputs = read_ports(ui.get_function_editor_outputs());
            if let Err(message) = validate_ports("输入", &inputs) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }
            if let Err(message) = validate_ports("输出", &outputs) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }

            let node_id = ui.get_function_editor_node_id().to_string();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node_index) = tab.graph.nodes.iter().position(|node| node.id == node_id) {
                    let node = &tab.graph.nodes[node_index];
                    let mut config = embedded_function_config_from_node(node)
                        .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));
                    config.name = name;
                    config.description = ui.get_function_editor_description().to_string();
                    config.inputs = ports_from_vm(&inputs);
                    config.outputs = ports_from_vm(&outputs);

                    let node = &mut tab.graph.nodes[node_index];
                    sync_function_node_definition(node, &config);
                    let input_names: HashSet<&str> =
                        node.input_ports.iter().map(|port| port.name.as_str()).collect();
                    let output_names: HashSet<&str> =
                        node.output_ports.iter().map(|port| port.name.as_str()).collect();
                    tab.graph.edges.retain(|edge| {
                        let input_ok = if edge.to_node_id == node.id {
                            input_names.contains(edge.to_port.as_str())
                        } else {
                            true
                        };
                        let output_ok = if edge.from_node_id == node.id {
                            output_names.contains(edge.from_port.as_str())
                        } else {
                            true
                        };
                        input_ok && output_ok
                    });
                    if let Ok(value) = serde_json::to_value(&config) {
                        tab.inline_inputs.insert(
                            inline_port_key(&node.id, FUNCTION_CONFIG_PORT),
                            InlinePortValue::Json(value),
                        );
                    }
                    tab.is_dirty = true;
                }
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_function_editor_dialog(false);
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_enter_function_subgraph_clicked(move |node_id| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let mut next_canvas_state = None;
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let Some(node) = tab.graph.nodes.iter().find(|node| node.id == node_id.as_str()) else {
                    return;
                };
                let config = embedded_function_config_from_node(node)
                    .unwrap_or_else(|| default_embedded_function_config(node.name.clone()));
                tab.push_subgraph_page(
                    SubgraphOwner::FunctionNode {
                        node_id: node_id.to_string(),
                    },
                    config.name.clone(),
                    config.subgraph.clone(),
                );
                next_canvas_state = Some(tab.canvas_view_state.clone());
            }
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            if let Some(state) = next_canvas_state.as_ref() {
                apply_canvas_view_state(&ui, state);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_navigate_subgraph_back(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let mut next_canvas_state = None;
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if tab.pop_subgraph_page() {
                    next_canvas_state = Some(tab.canvas_view_state.clone());
                }
            }
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            if let Some(state) = next_canvas_state.as_ref() {
                apply_canvas_view_state(&ui, state);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_navigate_subgraph_root(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let mut next_canvas_state = None;
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if tab.return_to_root_page() {
                    next_canvas_state = Some(tab.canvas_view_state.clone());
                }
            }
            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            if let Some(state) = next_canvas_state.as_ref() {
                apply_canvas_view_state(&ui, state);
            }
        });
    }
}
