use std::path::PathBuf;

use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::graph_boundary::{root_graph_to_tool_subgraph, sync_root_graph_io};
use zihuan_graph_engine::DataType;
use zihuan_llm::brain_tool::{
    fixed_tool_runtime_inputs, BrainToolDefinition, ToolParamDef, QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_llm::system_config::{AgentToolConfig, AgentToolType, NodeGraphToolConfig};

const LEGACY_QQ_AGENT_TOOL_OWNER_TYPE: &str = "qq_message_agent";

pub fn build_enabled_tool_definitions(
    tools: &[AgentToolConfig],
) -> Result<Vec<BrainToolDefinition>> {
    let mut definitions = Vec::new();
    for tool in tools.iter().filter(|tool| tool.enabled) {
        match &tool.tool_type {
            AgentToolType::NodeGraph(config) => {
                definitions.push(build_node_graph_tool_definition(tool, config)?);
            }
        }
    }
    Ok(definitions)
}

fn build_node_graph_tool_definition(
    tool: &AgentToolConfig,
    config: &NodeGraphToolConfig,
) -> Result<BrainToolDefinition> {
    let (mut graph, parameters, outputs) = match config {
        NodeGraphToolConfig::FilePath {
            path,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from(path))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::WorkflowSet {
            name,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from("workflow_set").join(format!("{name}.json")))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::InlineGraph {
            graph,
            parameters,
            outputs,
        } => (graph.clone(), parameters.clone(), outputs.clone()),
    };

    sync_root_graph_io(&mut graph);
    validate_tool_graph_contract(tool, &graph, &parameters, &outputs)?;
    let subgraph = root_graph_to_tool_subgraph(&graph);
    let parameters = merge_parameter_descriptions_from_graph(&parameters, &graph.graph_inputs);
    let outputs = merge_output_descriptions_from_graph(&outputs, &graph.graph_outputs);

    Ok(BrainToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters,
        outputs,
        subgraph,
    })
}

fn load_graph_from_path(
    path: PathBuf,
) -> Result<zihuan_graph_engine::graph_io::NodeGraphDefinition> {
    if !path.exists() {
        return Err(Error::ValidationError(format!(
            "tool graph file not found: {}",
            path.display()
        )));
    }
    let loaded = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&path)?;
    Ok(loaded.graph)
}

fn validate_tool_graph_contract(
    tool: &AgentToolConfig,
    graph: &zihuan_graph_engine::graph_io::NodeGraphDefinition,
    parameters: &[ToolParamDef],
    outputs: &[FunctionPortDef],
) -> Result<()> {
    if graph.graph_inputs.is_empty() {
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
    for owner_type in [
        "brain",
        QQ_AGENT_TOOL_OWNER_TYPE,
        LEGACY_QQ_AGENT_TOOL_OWNER_TYPE,
    ] {
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
            let graph_description = inputs
                .iter()
                .find(|input| {
                    reserved_tool_graph_input_type(&input.name).is_none()
                        && input.name.trim() == param.name.trim()
                        && input.data_type == param.data_type
                })
                .map(|input| input.description.trim())
                .filter(|description| !description.is_empty());

            let mut merged = param.clone();
            if let Some(description) = graph_description {
                merged.desc = description.to_string();
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
