use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::llm::brain_tool::{
    brain_shared_inputs_from_value, brain_tool_input_signature, BrainToolDefinition, ToolParamDef,
    BRAIN_SHARED_INPUTS_PORT, BRAIN_TOOLS_CONFIG_PORT, BRAIN_TOOL_FIXED_CONTENT_INPUT,
};
use crate::node::function_graph::{sync_function_subgraph_signature, FunctionPortDef};
use crate::node::DataType;
use crate::node::Port;
use crate::ui::graph_window::{FunctionPortVm, NodeGraphWindow, ToolDefinitionVm, ToolParamVm};
use crate::ui::node_graph_view::{
    apply_canvas_view_state, refresh_active_tab_ui, GraphTabState, SubgraphOwner,
};
use crate::ui::node_render::{inline_port_key, InlinePortValue};

fn default_param_vm() -> ToolParamVm {
    ToolParamVm {
        name: "param".into(),
        data_type: "String".into(),
        description: "".into(),
        optional: false,
    }
}

fn default_shared_input_vm() -> FunctionPortVm {
    FunctionPortVm {
        name: "shared_input".into(),
        data_type: "String".into(),
    }
}

fn default_tool_vm(index: usize) -> ToolDefinitionVm {
    ToolDefinitionVm {
        id: format!("tool_{index}").into(),
        name: format!("tool_{index}").into(),
        description: "".into(),
        params: ModelRc::new(VecModel::from(vec![default_param_vm()])),
        outputs: ModelRc::new(VecModel::from(Vec::<FunctionPortVm>::new())),
    }
}

fn next_default_tool_index(items: &[ToolDefinitionVm]) -> usize {
    let existing_ids = items
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let existing_names = items
        .iter()
        .map(|item| item.name.as_str())
        .collect::<HashSet<_>>();

    let mut index = 1usize;
    loop {
        let candidate = format!("tool_{index}");
        if !existing_ids.contains(candidate.as_str()) && !existing_names.contains(candidate.as_str())
        {
            return index;
        }
        index += 1;
    }
}

fn default_output_vm() -> FunctionPortVm {
    FunctionPortVm {
        name: "result".into(),
        data_type: "String".into(),
    }
}

fn string_to_data_type(value: &str) -> DataType {
    serde_json::from_value::<DataType>(Value::String(value.to_string())).unwrap_or(DataType::String)
}

fn shared_inputs_from_json(value: &serde_json::Value) -> Vec<FunctionPortVm> {
    brain_shared_inputs_from_value(value)
        .unwrap_or_default()
        .into_iter()
        .map(|port| FunctionPortVm {
            name: port.name.into(),
            data_type: port.data_type.to_string().into(),
        })
        .collect()
}

fn tool_items_from_json(value: &serde_json::Value) -> Vec<ToolDefinitionVm> {
    serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|tool| ToolDefinitionVm {
            id: tool.id.into(),
            name: tool.name.into(),
            description: tool.description.into(),
            params: ModelRc::new(VecModel::from(
                tool.parameters
                    .into_iter()
                    .map(|param| ToolParamVm {
                        name: param.name.into(),
                        data_type: param.data_type.to_string().into(),
                        description: param.desc.into(),
                        optional: param.optional,
                    })
                    .collect::<Vec<_>>(),
            )),
            outputs: ModelRc::new(VecModel::from(
                tool.outputs
                    .into_iter()
                    .map(|output| FunctionPortVm {
                        name: output.name.into(),
                        data_type: output.data_type.to_string().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
        })
        .collect()
}

fn read_tool_items(ui: &NodeGraphWindow) -> Vec<ToolDefinitionVm> {
    ui.get_tool_editor_items().iter().collect()
}

fn read_shared_inputs(ui: &NodeGraphWindow) -> Vec<FunctionPortVm> {
    ui.get_tool_editor_shared_inputs().iter().collect()
}

fn write_shared_inputs(ui: &NodeGraphWindow, shared_inputs: Vec<FunctionPortVm>) {
    ui.set_tool_editor_shared_inputs(ModelRc::new(VecModel::from(shared_inputs)));
}

fn replace_shared_input_row(ui: &NodeGraphWindow, index: usize, new_item: FunctionPortVm) {
    let model = ui.get_tool_editor_shared_inputs();
    if index < model.row_count() {
        model.set_row_data(index, new_item);
    }
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

fn params_from_vm(params: ModelRc<ToolParamVm>) -> Vec<ToolParamDef> {
    params
        .iter()
        .map(|param| ToolParamDef {
            name: param.name.to_string(),
            data_type: string_to_data_type(param.data_type.as_str()),
            desc: param.description.to_string(),
            optional: param.optional,
        })
        .collect()
}

fn outputs_from_vm(outputs: ModelRc<FunctionPortVm>) -> Vec<FunctionPortDef> {
    outputs
        .iter()
        .map(|output| FunctionPortDef {
            name: output.name.to_string(),
            data_type: string_to_data_type(output.data_type.as_str()),
        })
        .collect()
}

fn shared_inputs_from_vm(shared_inputs: &[FunctionPortVm]) -> Vec<FunctionPortDef> {
    shared_inputs
        .iter()
        .map(|port| FunctionPortDef {
            name: port.name.to_string(),
            data_type: string_to_data_type(port.data_type.as_str()),
        })
        .collect()
}

fn validate_tools(shared_inputs: &[FunctionPortVm], items: &[ToolDefinitionVm]) -> Result<(), String> {
    let mut shared_names = HashSet::new();
    for input in shared_inputs {
        let name = input.name.as_str().trim();
        if name.is_empty() {
            return Err("共享输入名称不能为空".to_string());
        }
        if !shared_names.insert(name.to_string()) {
            return Err(format!("共享输入名称重复：{}", name));
        }
    }

    let mut tool_ids = HashSet::new();
    let mut tool_names = HashSet::new();

    for tool in items {
        let tool_id = tool.id.as_str().trim();
        let tool_name = tool.name.as_str().trim();
        if tool_id.is_empty() {
            return Err("Tool 内部 ID 不能为空".to_string());
        }
        if tool_name.is_empty() {
            return Err("Tool 名称不能为空".to_string());
        }
        if !tool_ids.insert(tool_id.to_string()) {
            return Err(format!("Tool ID 重复：{}", tool_id));
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
            if shared_names.contains(param_name) {
                return Err(format!(
                    "Tool '{}' 的参数名与共享输入重复：{}",
                    tool_name, param_name
                ));
            }
            if param_name == BRAIN_TOOL_FIXED_CONTENT_INPUT {
                return Err(format!(
                    "Tool '{}' 的参数名不能使用保留名称：{}",
                    tool_name, param_name
                ));
            }
            if !param_names.insert(param_name.to_string()) {
                return Err(format!("Tool '{}' 的参数名重复：{}", tool_name, param_name));
            }
        }

        let mut output_names = HashSet::new();
        for output in tool.outputs.iter() {
            let output_name = output.name.as_str().trim();
            if output_name.is_empty() {
                return Err(format!("Tool '{}' 存在空输出名", tool_name));
            }
            if !output_names.insert(output_name.to_string()) {
                return Err(format!("Tool '{}' 的输出名重复：{}", tool_name, output_name));
            }
        }
    }

    Ok(())
}

fn brain_output_ports() -> Vec<Port> {
    vec![Port::new("output", DataType::Vec(Box::new(DataType::OpenAIMessage)))
        .with_description("本次 Brain 运行新增的 assistant/tool 消息轨迹")]
}

fn merge_tool_items_with_existing(
    shared_inputs: &[FunctionPortDef],
    existing: &[BrainToolDefinition],
    items: &[ToolDefinitionVm],
) -> Vec<BrainToolDefinition> {
    let existing_by_id: HashMap<String, BrainToolDefinition> = existing
        .iter()
        .cloned()
        .map(|tool| (tool.id.clone(), tool))
        .collect();

    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let mut tool = existing_by_id
                .get(item.id.as_str())
                .cloned()
                .unwrap_or_else(BrainToolDefinition::default);
            tool.id = item.id.to_string();
            tool.name = item.name.to_string();
            tool.description = item.description.to_string();
            tool.parameters = params_from_vm(item.params.clone());
            tool.outputs = outputs_from_vm(item.outputs.clone());
            tool.ensure_defaults(index + 1);
            let input_signature = brain_tool_input_signature(shared_inputs, &tool);
            sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &tool.outputs);
            tool
        })
        .collect()
}

fn persist_tool_editor_items(
    tab: &mut GraphTabState,
    node_id: &str,
    shared_input_items: &[FunctionPortVm],
    items: &[ToolDefinitionVm],
) -> Option<Vec<BrainToolDefinition>> {
    let node = tab.graph.nodes.iter_mut().find(|node| node.id == node_id)?;
    let shared_inputs = shared_inputs_from_vm(shared_input_items);
    let existing = node
        .inline_values
        .get(BRAIN_TOOLS_CONFIG_PORT)
        .and_then(|value| serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone()).ok())
        .unwrap_or_default();
    let tools = merge_tool_items_with_existing(&shared_inputs, &existing, items);
    let tools_json = serde_json::to_value(&tools).ok()?;
    let shared_inputs_json = serde_json::to_value(&shared_inputs).ok()?;

    node.inline_values
        .insert(BRAIN_TOOLS_CONFIG_PORT.to_string(), tools_json.clone());
    node.inline_values
        .insert(BRAIN_SHARED_INPUTS_PORT.to_string(), shared_inputs_json.clone());
    node.input_ports = vec![
        Port::new("llm_model", DataType::LLModel)
            .with_description("LLM 模型引用，由 LLM API 节点提供"),
        Port::new("messages", DataType::Vec(Box::new(DataType::OpenAIMessage)))
            .with_description("消息列表（包含 system/user/assistant/tool 等角色）"),
        Port::new(BRAIN_TOOLS_CONFIG_PORT, DataType::Json)
            .with_description("Tools 配置，由工具编辑器维护")
            .optional(),
        Port::new(BRAIN_SHARED_INPUTS_PORT, DataType::Json)
            .with_description("Brain 共享输入签名，由工具编辑器维护")
            .optional(),
    ];
    node.input_ports.extend(shared_inputs.iter().map(|port| {
        Port::new(port.name.clone(), port.data_type.clone())
            .with_description(format!("Brain 共享输入 '{}'", port.name))
    }));
    node.dynamic_input_ports = true;
    node.output_ports = brain_output_ports();
    node.dynamic_output_ports = false;
    tab.graph
        .edges
        .retain(|edge| edge.from_node_id != node.id || edge.from_port == "output");
    tab.inline_inputs.insert(
        inline_port_key(&node.id, BRAIN_TOOLS_CONFIG_PORT),
        InlinePortValue::Json(tools_json),
    );
    tab.inline_inputs.insert(
        inline_port_key(&node.id, BRAIN_SHARED_INPUTS_PORT),
        InlinePortValue::Json(shared_inputs_json),
    );
    tab.is_dirty = true;

    Some(tools)
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
                .get(BRAIN_TOOLS_CONFIG_PORT)
                .map(tool_items_from_json)
                .unwrap_or_default();
            let shared_inputs = node
                .inline_values
                .get(BRAIN_SHARED_INPUTS_PORT)
                .map(shared_inputs_from_json)
                .unwrap_or_default();

            ui.set_tool_editor_node_id(node_id);
            ui.set_tool_editor_selected_index(if items.is_empty() { -1 } else { 0 });
            write_shared_inputs(&ui, shared_inputs);
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
                write_shared_inputs(&ui, Vec::new());
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
        ui.on_tool_editor_add_shared_input(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut shared_inputs = read_shared_inputs(&ui);
                shared_inputs.push(default_shared_input_vm());
                write_shared_inputs(&ui, shared_inputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_delete_shared_input(move |index| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let mut shared_inputs = read_shared_inputs(&ui);
                if idx < shared_inputs.len() {
                    shared_inputs.remove(idx);
                }
                write_shared_inputs(&ui, shared_inputs);
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_shared_input_name(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_tool_editor_shared_inputs();
                if let Some(mut input) = model.row_data(idx) {
                    input.name = value;
                    replace_shared_input_row(&ui, idx, input);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_shared_input_type(move |index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = index.max(0) as usize;
                let model = ui.get_tool_editor_shared_inputs();
                if let Some(mut input) = model.row_data(idx) {
                    input.data_type = value;
                    replace_shared_input_row(&ui, idx, input);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_add_tool(move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut items = read_tool_items(&ui);
                let next_index = next_default_tool_index(&items);
                items.push(default_tool_vm(next_index));
                let selected_index = (items.len() - 1) as i32;
                write_tool_items(&ui, items);
                ui.set_tool_editor_selected_index(selected_index);
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
                if let Some(item) = model.row_data(idx) {
                    let params_model = item.params.clone();
                    let mut params: Vec<ToolParamVm> = params_model.iter().collect();
                    params.push(default_param_vm());
                    let mut new_item = item;
                    new_item.params = ModelRc::new(VecModel::from(params));
                    replace_tool_row(&ui, idx, new_item);
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
                if let Some(item) = model.row_data(t_idx) {
                    let mut params: Vec<ToolParamVm> = item.params.iter().collect();
                    if p_idx < params.len() {
                        params.remove(p_idx);
                    }
                    let mut new_item = item;
                    new_item.params = ModelRc::new(VecModel::from(params));
                    replace_tool_row(&ui, t_idx, new_item);
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
        ui.on_tool_editor_set_param_description(move |tool_index, param_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let p_idx = param_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let params_model = item.params.clone();
                    if let Some(mut param) = params_model.row_data(p_idx) {
                        param.description = value;
                        params_model.set_row_data(p_idx, param);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_param_optional(move |tool_index, param_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let p_idx = param_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let params_model = item.params.clone();
                    if let Some(mut param) = params_model.row_data(p_idx) {
                        param.optional = value;
                        params_model.set_row_data(p_idx, param);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_add_output(move |tool_index| {
            if let Some(ui) = ui_handle.upgrade() {
                let idx = tool_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(idx) {
                    let mut outputs: Vec<FunctionPortVm> = item.outputs.iter().collect();
                    outputs.push(default_output_vm());
                    let mut new_item = item;
                    new_item.outputs = ModelRc::new(VecModel::from(outputs));
                    replace_tool_row(&ui, idx, new_item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_delete_output(move |tool_index, output_index| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let o_idx = output_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let mut outputs: Vec<FunctionPortVm> = item.outputs.iter().collect();
                    if o_idx < outputs.len() {
                        outputs.remove(o_idx);
                    }
                    let mut new_item = item;
                    new_item.outputs = ModelRc::new(VecModel::from(outputs));
                    replace_tool_row(&ui, t_idx, new_item);
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_output_name(move |tool_index, output_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let o_idx = output_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let outputs_model = item.outputs.clone();
                    if let Some(mut output) = outputs_model.row_data(o_idx) {
                        output.name = value;
                        outputs_model.set_row_data(o_idx, output);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        ui.on_tool_editor_set_output_type(move |tool_index, output_index, value| {
            if let Some(ui) = ui_handle.upgrade() {
                let t_idx = tool_index.max(0) as usize;
                let o_idx = output_index.max(0) as usize;
                let model = ui.get_tool_editor_items();
                if let Some(item) = model.row_data(t_idx) {
                    let outputs_model = item.outputs.clone();
                    if let Some(mut output) = outputs_model.row_data(o_idx) {
                        output.data_type = value;
                        outputs_model.set_row_data(o_idx, output);
                    }
                }
            }
        });
    }

    {
        let ui_handle = ui.as_weak();
        let tabs_clone = Arc::clone(&tabs);
        let active_tab_clone = Arc::clone(&active_tab_index);
        ui.on_tool_editor_open_subgraph(move |tool_index| {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let items = read_tool_items(&ui);
            let shared_inputs = read_shared_inputs(&ui);
            if let Err(message) = validate_tools(&shared_inputs, &items) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }

            let node_id = ui.get_tool_editor_node_id().to_string();
            let tool_index = tool_index.max(0) as usize;

            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            let mut next_canvas_state = None;
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                let Some(saved_tools) =
                    persist_tool_editor_items(tab, &node_id, &shared_inputs, &items)
                else {
                    return;
                };
                let Some(tool) = saved_tools.get(tool_index) else {
                    return;
                };
                tab.push_subgraph_page(
                    SubgraphOwner::BrainTool {
                        node_id: node_id.clone(),
                        tool_id: tool.id.clone(),
                    },
                    tool.name.clone(),
                    tool.subgraph.clone(),
                );
                next_canvas_state = Some(tab.canvas_view_state.clone());
                ui.set_show_tool_editor_dialog(false);
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
        ui.on_save_tool_editor(move || {
            let Some(ui) = ui_handle.upgrade() else {
                return;
            };

            let items = read_tool_items(&ui);
            let shared_inputs = read_shared_inputs(&ui);
            if let Err(message) = validate_tools(&shared_inputs, &items) {
                ui.set_error_dialog_message(message.into());
                ui.set_show_error_dialog(true);
                return;
            }

            let node_id = ui.get_tool_editor_node_id().to_string();
            let mut tabs_guard = tabs_clone.lock().unwrap();
            let active_index = *active_tab_clone.lock().unwrap();
            if let Some(tab) = tabs_guard.get_mut(active_index) {
                persist_tool_editor_items(tab, &node_id, &shared_inputs, &items);
            }

            refresh_active_tab_ui(&ui, &tabs_guard, active_index);
            ui.set_show_tool_editor_dialog(false);
        });
    }
}
