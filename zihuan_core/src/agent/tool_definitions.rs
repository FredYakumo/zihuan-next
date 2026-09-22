use std::path::PathBuf;

use crate::agent::tool_config::{
    AgentToolConfig, AgentToolType, NodeGraphToolConfig, ScriptAgentToolConfig,
};
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::graph_boundary::{root_graph_to_tool_subgraph, sync_root_graph_io};
use crate::graph::script_tool::validate_script_tool_config;
use crate::graph::tool_spec::{
    fixed_tool_runtime_inputs, ToolDefinition, ToolImplementation, ToolParamDef,
    QQ_AGENT_TOOL_OWNER_TYPE,
};
use crate::graph::DataType;

pub fn build_enabled_tool_definitions(tools: &[AgentToolConfig]) -> Result<Vec<ToolDefinition>> {
    let mut definitions = Vec::new();
    for tool in tools.iter().filter(|tool| tool.enabled) {
        match &tool.tool_type {
            AgentToolType::NodeGraph(config) => {
                definitions.push(build_node_graph_tool_definition(tool, config)?);
            }
            AgentToolType::Script(config) => {
                definitions.push(build_script_tool_definition(tool, config)?);
            }
            AgentToolType::SubAgent(_) => {}
        }
    }
    Ok(definitions)
}

/// The enabled sub-agent ids a configuration's tools select.
///
/// Sub-agents are not [`ToolDefinition`]s: the owning runtime publishes them through its
/// [`AgentHost`](crate::agent::declarative_agent::AgentHost) instead, so they are resolved
/// separately from [`build_enabled_tool_definitions`]. Selecting one twice is collapsed to a
/// single entry, because a repeated id would register the same LLM-callable tool twice.
pub fn selected_sub_agent_ids(tools: &[AgentToolConfig]) -> Vec<String> {
    let mut ids = Vec::new();
    for tool in tools.iter().filter(|tool| tool.enabled) {
        if let AgentToolType::SubAgent(config) = &tool.tool_type {
            if !ids.contains(&config.sub_agent_id) {
                ids.push(config.sub_agent_id.clone());
            }
        }
    }
    ids
}

fn build_node_graph_tool_definition(
    tool: &AgentToolConfig,
    config: &NodeGraphToolConfig,
) -> Result<ToolDefinition> {
    let (mut graph, parameters, outputs) = match config {
        NodeGraphToolConfig::FilePath { path, parameters, outputs } => {
            (load_graph_from_path(PathBuf::from(path))?, parameters.clone(), outputs.clone())
        }
        NodeGraphToolConfig::WorkflowSet { name, parameters, outputs } => (
            load_graph_from_path(PathBuf::from("workflow_set").join(format!("{name}.json")))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::InlineGraph { graph, parameters, outputs } => {
            (graph.clone(), parameters.clone(), outputs.clone())
        }
    };

    sync_root_graph_io(&mut graph);
    let parameters = if parameters.is_empty() {
        derive_parameters_from_graph_inputs(&graph.graph_inputs)
    } else {
        parameters
    };
    let outputs = if outputs.is_empty() {
        graph.graph_outputs.clone()
    } else {
        outputs
    };

    validate_tool_graph_contract(tool, &graph, &parameters, &outputs)?;
    let subgraph = root_graph_to_tool_subgraph(&graph);
    let parameters = merge_parameter_descriptions_from_graph(&parameters, &graph.graph_inputs);
    let outputs = merge_output_descriptions_from_graph(&outputs, &graph.graph_outputs);

    Ok(ToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        run_duration: tool.run_duration,
        implementation: ToolImplementation::NodeGraph,
        built_in_kind: None,
        script_config: None,
        parameters,
        outputs,
        subgraph,
    })
}

/// Builds the runtime definition of a script tool.
///
/// Unlike a graph tool, the signature is not derived from anything on disk: the configuration
/// carries both the script and the parameters/outputs the LLM sees, so a service stays valid
/// without the script ever having been imported.
fn build_script_tool_definition(
    tool: &AgentToolConfig,
    config: &ScriptAgentToolConfig,
) -> Result<ToolDefinition> {
    validate_script_tool_config(&tool.name, &config.script)?;
    if config.outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 未定义 outputs，脚本必须声明至少一个输出",
            tool.name
        )));
    }
    if config.outputs.iter().any(|output| output.name.trim().is_empty()) {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 的 outputs 包含空的输出名",
            tool.name
        )));
    }

    Ok(ToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        run_duration: tool.run_duration,
        implementation: ToolImplementation::Script,
        built_in_kind: None,
        script_config: Some(config.script.clone()),
        parameters: config.parameters.clone(),
        outputs: config.outputs.clone(),
        subgraph: Default::default(),
    })
}

fn derive_parameters_from_graph_inputs(inputs: &[FunctionPortDef]) -> Vec<ToolParamDef> {
    inputs
        .iter()
        .filter(|input| reserved_tool_graph_input_type(&input.name).is_none())
        .map(|input| ToolParamDef {
            name: input.name.clone(),
            data_type: input.data_type.clone(),
            desc: input.description.clone(),
            required: input.required,
        })
        .collect()
}

fn load_graph_from_path(path: PathBuf) -> Result<crate::graph::graph_io::NodeGraphDefinition> {
    if !path.exists() {
        return Err(Error::ValidationError(format!(
            "tool graph file not found: {}",
            path.display()
        )));
    }
    crate::graph::load_graph_definition_from_json(&path)
}

fn validate_tool_graph_contract(
    tool: &AgentToolConfig,
    graph: &crate::graph::graph_io::NodeGraphDefinition,
    parameters: &[ToolParamDef],
    outputs: &[FunctionPortDef],
) -> Result<()> {
    let has_effective_inputs = !graph.graph_inputs.is_empty() || graph.accepts_agent_events;
    if !has_effective_inputs {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输入列表",
            tool.name
        )));
    }
    if graph.graph_outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输出列表",
            tool.name
        )));
    }
    if outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 未定义 outputs，必须与节点图输出匹配",
            tool.name
        )));
    }

    for port in &graph.graph_inputs {
        validate_tool_graph_input_port(tool, port)?;
    }

    if !same_param_signature(parameters, &graph.graph_inputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 parameters 与节点图输入定义不匹配",
            tool.name
        )));
    }
    if !same_port_signature(outputs, &graph.graph_outputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 outputs 与节点图输出定义不匹配",
            tool.name
        )));
    }

    Ok(())
}

fn validate_tool_graph_input_port(tool: &AgentToolConfig, port: &FunctionPortDef) -> Result<()> {
    if let Some(expected_type) = reserved_tool_graph_input_type(&port.name) {
        if port.data_type != expected_type {
            return Err(Error::ValidationError(format!(
                "agent tool '{}' 的保留输入 '{}' 类型不匹配：期望 {}，实际为 {}",
                tool.name, port.name, expected_type, port.data_type
            )));
        }
        return Ok(());
    }

    if matches!(
        port.data_type,
        DataType::Integer | DataType::Float | DataType::String | DataType::Boolean
    ) {
        return Ok(());
    }

    Err(Error::ValidationError(format!(
        "agent tool '{}' 的节点图输入 '{}' 类型必须是基础类型 int/float/string/boolean，或受支持的保留运行时输入；实际为 {}",
        tool.name, port.name, port.data_type
    )))
}

fn reserved_tool_graph_input_type(name: &str) -> Option<DataType> {
    let trimmed = name.trim();
    for owner_type in ["tool_calling", QQ_AGENT_TOOL_OWNER_TYPE] {
        for port in fixed_tool_runtime_inputs(owner_type) {
            if port.name == trimmed {
                return Some(port.data_type);
            }
        }
    }
    None
}

fn same_param_signature(parameters: &[ToolParamDef], inputs: &[FunctionPortDef]) -> bool {
    let exposed_inputs = inputs
        .iter()
        .filter(|input| reserved_tool_graph_input_type(&input.name).is_none())
        .collect::<Vec<_>>();

    parameters.len() == exposed_inputs.len()
        && parameters.iter().zip(exposed_inputs).all(|(param, input)| {
            param.name.trim() == input.name.trim() && param.data_type == input.data_type
        })
}

fn same_port_signature(left: &[FunctionPortDef], right: &[FunctionPortDef]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.name.trim() == b.name.trim() && a.data_type == b.data_type)
}

fn merge_parameter_descriptions_from_graph(
    parameters: &[ToolParamDef],
    inputs: &[FunctionPortDef],
) -> Vec<ToolParamDef> {
    parameters
        .iter()
        .map(|param| {
            let graph_input = inputs.iter().find(|input| {
                reserved_tool_graph_input_type(&input.name).is_none()
                    && input.name.trim() == param.name.trim()
                    && input.data_type == param.data_type
            });

            let mut merged = param.clone();
            if let Some(description) = graph_input
                .map(|input| input.description.trim())
                .filter(|description| !description.is_empty())
            {
                merged.desc = description.to_string();
            }
            if let Some(graph_input) = graph_input {
                merged.required = graph_input.required;
            }
            merged
        })
        .collect()
}

fn merge_output_descriptions_from_graph(
    outputs: &[FunctionPortDef],
    graph_outputs: &[FunctionPortDef],
) -> Vec<FunctionPortDef> {
    outputs
        .iter()
        .map(|output| {
            let mut merged = output.clone();
            if let Some(description) = graph_outputs
                .iter()
                .find(|graph_output| {
                    graph_output.name.trim() == output.name.trim()
                        && graph_output.data_type == output.data_type
                })
                .map(|graph_output| graph_output.description.trim())
                .filter(|description| !description.is_empty())
            {
                merged.description = description.to_string();
            }
            merged
        })
        .collect()
}
