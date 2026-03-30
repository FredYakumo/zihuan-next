use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::node::DataType;
use crate::node::Port;
use crate::ui::graph_window::{NodeGraphWindow, ToolDefinitionVm, ToolParamVm};
use crate::ui::node_graph_view::{GraphTabState, refresh_active_tab_ui};
use crate::ui::node_render::{InlinePortValue, inline_port_key};

const TOOLS_CONFIG_PORT: &str = "tools_config";

fn default_param_vm() -> ToolParamVm {
    ToolParamVm {
        name: "param".into(),
        data_type: "String".into(),
    }
}

fn default_tool_vm() -> ToolDefinitionVm {
    ToolDefinitionVm {
        name: "new_tool".into(),
        description: "".into(),
        terminal_on_success: false,
        params: ModelRc::new(VecModel::from(vec![default_param_vm()])),
    }
}

fn tool_items_from_json(value: &serde_json::Value) -> Vec<ToolDefinitionVm> {
    value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|tool| tool.as_object().cloned())
        .map(|tool| {
            let params = tool
                .get("parameters")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|param| param.as_object().cloned())
                .map(|param| ToolParamVm {
                    name: param
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    data_type: param
                        .get("data_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("String")
                        .into(),
                })
                .collect::<Vec<_>>();

            ToolDefinitionVm {
                name: tool
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                description: tool
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                terminal_on_success: tool
                    .get("terminal_on_success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                params: ModelRc::new(VecModel::from(params)),
            }
        })
        .collect()
}

fn params_to_json(params: ModelRc<ToolParamVm>) -> Vec<serde_json::Value> {
    params
        .iter()
        .map(|param| {
            serde_json::json!({
                "name": param.name.as_str(),
                "data_type": param.data_type.as_str(),
            })
        })
        .collect()
}

fn tool_items_to_json(items: &[ToolDefinitionVm]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name.as_str(),
                    "description": tool.description.as_str(),
                    "terminal_on_success": tool.terminal_on_success,
                    "parameters": params_to_json(tool.params.clone()),
                })
            })
            .collect(),
    )
}

fn read_tool_items(ui: &NodeGraphWindow) -> Vec<ToolDefinitionVm> {
    ui.get_tool_editor_items().iter().collect()
}

fn write_tool_items(ui: &NodeGraphWindow, items: Vec<ToolDefinitionVm>) {
    ui.set_tool_editor_items(ModelRc::new(VecModel::from(items)));
}

fn replace_tool_row(ui: &NodeGraphWindow, index: usize, new_item: ToolDefinitionVm) {
    let model = ui.get_tool_editor_items();
    if index < model.row_count() {
        model.set_row_data(index, new_item);
    }
}

fn validate_tools(items: &[ToolDefinitionVm]) -> Result<(), String> {
    let mut tool_names = HashSet::new();

    for tool in items {
        let tool_name = tool.name.as_str().trim();
        if tool_name.is_empty() {
            return Err("Tool 名称不能为空".to_string());
        }
        if !tool_names.insert(tool_name.to_string()) {
            return Err(format!("Tool 名称重复：{}", tool_name));
        }

        let mut param_names = HashSet::new();
        for param in tool.params.iter() {
            let param_name = param.name.as_str().trim();
            if param_name.is_empty() {
                return Err(format!("Tool '{}' 存在空参数名", tool_name));
            }
            if !param_names.insert(param_name.to_string()) {
                return Err(format!("Tool '{}' 的参数名重复：{}", tool_name, param_name));
            }
        }
    }

    Ok(())
}

fn brain_output_ports(items: &[ToolDefinitionVm]) -> Vec<Port> {
    let mut ports = vec![
        Port::new("assistant_message", DataType::OpenAIMessage)
            .with_description("LLM 返回的完整 assistant 消息（含 tool_calls，用于 agentic loop）"),
        Port::new("has_tool_call", DataType::Boolean)
            .with_description("LLM 返回的 assistant 消息是否包含 tool_calls，用于控制 agentic loop 继续或结束"),
        Port::new("terminal_tool_called", DataType::Boolean)
            .with_description("LLM 返回的 assistant 消息是否调用了标记为 terminal_on_success 的工具"),
    ];
    ports.extend(items.iter().map(|tool| {
        Port::new(tool.name.as_str(), DataType::Json)
            .with_description(format!("Tool '{}' 的调用参数 JSON", tool.name))
    }));
    ports
}

pub(crate) fn bind_tool_editor_callbacks(
    ui: &NodeGraphWindow,
    tabs: Arc<Mutex<Vec<GraphTabState>>>,
    active_tab_index: Arc<Mutex<usize>>,
) {
    {
        let ui_handle = ui.as_weak();
        ui.on_edit_tools_clicked(move |node_id| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.invoke_open_tool_editor(node_id);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_open_tool_editor(move |node_id| {
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
                .get(TOOLS_CONFIG_PORT)
                .map(tool_items_from_json)
                .unwrap_or_default();

            ui.set_tool_editor_node_id(node_id);
            ui.set_tool_editor_selected_index(if items.is_empty() { -1 } else { 0 });
            write_tool_items(&ui, items);
            ui.set_show_tool_editor_dialog(true);
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_close_tool_editor(move || {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_show_tool_editor_dialog(false);
                ui.set_tool_editor_node_id("".into());
                ui.set_tool_editor_selected_index(-1);
                write_tool_items(&ui, Vec::new());
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_select(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                ui.set_tool_editor_selected_index(index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_add_tool(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut items = read_tool_items(&ui);
                items.push(default_tool_vm());
                let next_index = items.len().saturating_sub(1) as i32;
                write_tool_items(&ui, items);
                ui.set_tool_editor_selected_index(next_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_delete_tool(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                let mut items = read_tool_items(&ui);
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
                write_tool_items(&ui, items);
                ui.set_tool_editor_selected_index(next_index);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_tool_name(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(mut item) = model.row_data(idx) {
                    item.name = value;
                    replace_tool_row(&ui, idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_tool_description(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(mut item) = model.row_data(idx) {
                    item.description = value;
                    replace_tool_row(&ui, idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_add_param(move |tool_index| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = tool_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(mut item) = model.row_data(idx) {
                    let mut params: Vec<ToolParamVm> = item.params.iter().collect();
                    params.push(default_param_vm());
                    item.params = ModelRc::new(VecModel::from(params));
                    replace_tool_row(&ui, idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_delete_param(move |tool_index, param_index| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let p_idx = param_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(mut item) = model.row_data(t_idx) {
                    let mut params: Vec<ToolParamVm> = item.params.iter().collect();
                    if p_idx < params.len() {
                        params.remove(p_idx);
                    }
                    item.params = ModelRc::new(VecModel::from(params));
                    replace_tool_row(&ui, t_idx, item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_param_name(move |tool_index, param_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let p_idx = param_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let params_model = item.params.clone();
                    if let Some(mut param) = params_model.row_data(p_idx) {
                        param.name = value;
                        params_model.set_row_data(p_idx, param);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_param_type(move |tool_index, param_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let p_idx = param_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let params_model = item.params.clone();
                    if let Some(mut param) = params_model.row_data(p_idx) {
                        param.data_type = value;
                        params_model.set_row_data(p_idx, param);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_save_tool_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let items = read_tool_items(&ui);
            if let Err(message) = validate_tools(&items) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }

            let tools_json = tool_items_to_json(&items);
            let node_id = ui.get_tool_editor_node_id();

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                if let Some(node) = tab.graph.nodes.iter_mut().find(|n| n.id == node_id.as_str()) {
                    node.inline_values.insert(TOOLS_CONFIG_PORT.to_string(), tools_json.clone());
                    node.output_ports = brain_output_ports(&items);
                    let output_names: HashSet<&str> = node.output_ports.iter().map(|p| p.name.as_str()).collect();
                    tab.graph.edges.retain(|edge| {
                        if edge.from_node_id == node.id {
                            output_names.contains(edge.from_port.as_str())
                        } else {
                            true
                        }
                    });
                    tab.inline_inputs.insert(
                        inline_port_key(&node.id, TOOLS_CONFIG_PORT),
                        InlinePortValue::Json(tools_json),
                    );
                    tab.is_dirty = true;
                }
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_tool_editor_dialog(false);
        });
    }
}
