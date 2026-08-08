use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::agent_config::qq_chat::QqChatAgentServiceConfig;
use crate::error::Result;
use crate::error::Error;
use crate::graph_engine::brain_tool_spec::{BrainToolDefinition, BrainToolImplementation, BuiltInBrainToolKind, PythonScriptToolConfig, fixed_tool_runtime_inputs};
use crate::graph_engine::function_graph::FunctionPortDef;
use crate::graph_engine::{DataType, DataValue};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultMode {
    JsonObject,
    SingleString,
}

#[derive(Debug, Clone)]
pub struct ToolSubgraphRunner {
    pub node_id: String,
    pub owner_node_type: String,
    pub shared_inputs: Vec<FunctionPortDef>,
    pub definition: BrainToolDefinition,
    pub shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
    pub qq_chat_agent_config: Option<QqChatAgentServiceConfig>,
    pub result_mode: ToolResultMode,
}

impl ToolSubgraphRunner {
    pub fn spec(&self) -> Arc<dyn crate::llm::tooling::FunctionTool> {
        Arc::new(SubgraphFunctionTool::new(self.definition.clone()))
    }

    pub fn execute_to_string(&self, _call_content: &str, _arguments: &serde_json::Value) -> String {
        // TODO: implement actual subgraph execution
        String::new()
    }
}

#[derive(Debug, Clone)]
struct SubgraphFunctionTool {
    definition: BrainToolDefinition,
}

impl SubgraphFunctionTool {
    fn new(definition: BrainToolDefinition) -> Self {
        Self { definition }
    }
}

impl crate::llm::tooling::FunctionTool for SubgraphFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::Value::Array(vec![])
    }

    fn call(&self, arguments: serde_json::Value) -> Result<serde_json::Value> {
        Ok(arguments)
    }
}

pub fn validate_shared_inputs(shared_inputs: &[FunctionPortDef], owner_label: &str) -> Result<Vec<FunctionPortDef>> {
    let mut seen_names = HashSet::new();
    let mut normalized = Vec::with_capacity(shared_inputs.len());

    for port in shared_inputs.iter().cloned() {
        let name = port.name.trim();
        if name.is_empty() {
            return Err(Error::ValidationError(format!(
                "{owner_label} shared input name cannot be empty"
            )));
        }
        if !seen_names.insert(name.to_string()) {
            return Err(Error::ValidationError(format!(
                "Duplicate {owner_label} shared input name: {name}"
            )));
        }
        normalized.push(FunctionPortDef {
            name: name.to_string(),
            data_type: port.data_type,
            description: port.description,
            required: port.required,
        });
    }

    Ok(normalized)
}

pub fn validate_tool_definitions(
    tool_definitions: &[BrainToolDefinition],
    shared_inputs: &[FunctionPortDef],
    result_mode: ToolResultMode,
    owner_node_type: &str,
    owner_label: &str,
) -> Result<Vec<BrainToolDefinition>> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let shared_input_names = shared_inputs
        .iter()
        .map(|port| port.name.trim().to_string())
        .collect::<HashSet<_>>();
    let mut normalized = Vec::with_capacity(tool_definitions.len());

    for (index, tool) in tool_definitions.iter().cloned().enumerate() {
        let mut tool = tool;
        tool.ensure_defaults(index + 1);

        let tool_id = tool.id.trim();
        let tool_name = tool.name.trim();
        if tool_id.is_empty() {
            return Err(Error::ValidationError("Tool id cannot be empty".to_string()));
        }
        if tool_name.is_empty() {
            return Err(Error::ValidationError("Tool name cannot be empty".to_string()));
        }
        if !seen_ids.insert(tool_id.to_string()) {
            return Err(Error::ValidationError(format!("Duplicate tool id: {tool_id}")));
        }
        if !seen_names.insert(tool_name.to_string()) {
            return Err(Error::ValidationError(format!("Duplicate tool name: {tool_name}")));
        }

        let mut seen_param_names = HashSet::new();
        let fixed_input_names = fixed_tool_runtime_inputs(owner_node_type)
            .into_iter()
            .map(|port| port.name)
            .collect::<HashSet<_>>();
        for param in &tool.parameters {
            let param_name = param.name.trim();
            if param_name.is_empty() {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' has an empty parameter name",
                    tool_name
                )));
            }
            if !seen_param_names.insert(param_name.to_string()) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' has duplicate parameter name: {param_name}",
                    tool_name
                )));
            }
            if fixed_input_names.contains(param_name) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' parameter '{}' shadows a system input",
                    tool_name, param_name
                )));
            }
            if shared_input_names.contains(param_name) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' parameter '{}' shadows a shared input",
                    tool_name, param_name
                )));
            }
        }

        normalized.push(tool);
    }

    Ok(normalized)
}
