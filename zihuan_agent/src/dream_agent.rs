use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::LLMMessage;
use zihuan_graph_engine::brain_tool_spec::{
    brain_tool_input_signature, BrainToolDefinition, ToolParamDef,
};
use zihuan_graph_engine::function_graph::{
    sync_function_subgraph_signature, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use zihuan_graph_engine::graph_io::refresh_port_types;
use zihuan_graph_engine::registry::build_node_graph_from_definition;
use zihuan_graph_engine::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use zihuan_graph_engine::{DataType, DataValue};

use crate::brain::{Brain, BrainStopReason, BrainTool, ToolExecutionOutput, ToolRunDuration};

const DREAM_SYSTEM_PROMPT: &str =
    "You are the Dream memory consolidation agent. Produce concise long-term memories in English. Do not address the user. Use the available node graph tools synchronously when they are relevant to consolidating the memory.";

struct DreamNodeGraphTool {
    definition: BrainToolDefinition,
}

impl DreamNodeGraphTool {
    fn new(definition: BrainToolDefinition) -> Self {
        Self { definition }
    }

    fn run_node_graph(&self, call_content: &str, arguments: &Value) -> Result<String> {
        let arguments = arguments.as_object().ok_or_else(|| {
            Error::ValidationError(format!(
                "Dream node graph tool '{}' requires JSON object arguments",
                self.definition.name
            ))
        })?;

        let mut runtime_values = HashMap::new();
        runtime_values.insert(
            "content".to_string(),
            DataValue::String(call_content.to_string()),
        );
        for parameter in &self.definition.parameters {
            let Some(value) = arguments.get(&parameter.name) else {
                if parameter.required {
                    return Err(Error::ValidationError(format!(
                        "Dream node graph tool '{}' is missing required parameter '{}'",
                        self.definition.name, parameter.name
                    )));
                }
                continue;
            };
            if value.is_null() && !parameter.required {
                continue;
            }
            let port = zihuan_graph_engine::function_graph::FunctionPortDef {
                name: parameter.name.clone(),
                data_type: parameter.data_type.clone(),
                description: parameter.desc.clone(),
                required: parameter.required,
            };
            runtime_values.insert(parameter.name.clone(), data_value_from_json_with_declared_type(&port, value)?);
        }

        let input_signature = brain_tool_input_signature("brain", &[], &self.definition);
        let mut subgraph = self.definition.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &self.definition.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "Dream node graph tool '{}' is missing the function_inputs boundary node",
                    self.definition.name
                ))
            })?;
        function_inputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "Dream node graph tool '{}' is missing the function_outputs boundary node",
                    self.definition.name
                ))
            })?;
        function_outputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&self.definition.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph).map_err(|error| {
            Error::ValidationError(format!(
                "Dream node graph tool '{}' could not build its subgraph: {error}",
                self.definition.name
            ))
        })?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values.into()).map_err(|error| {
            Error::ValidationError(format!(
                "Dream node graph tool '{}' could not inject runtime inputs: {error}",
                self.definition.name
            ))
        })?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(error) = execution_result.error_message {
            return Err(Error::ValidationError(format!(
                "Dream node graph tool '{}' failed: {error}",
                self.definition.name
            )));
        }

        let output_values = execution_result
            .node_results
            .get(FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "Dream node graph tool '{}' produced no function_outputs result",
                    self.definition.name
                ))
            })?;
        let mut result = Map::new();
        for output in &self.definition.outputs {
            let value = output_values.get(&output.name).ok_or_else(|| {
                Error::ValidationError(format!(
                    "Dream node graph tool '{}' did not provide output '{}'",
                    self.definition.name, output.name
                ))
            })?;
            if !output.data_type.is_compatible_with(&value.data_type()) {
                return Err(Error::ValidationError(format!(
                    "Dream node graph tool '{}' output '{}' type mismatch: expected {}, got {}",
                    self.definition.name,
                    output.name,
                    output.data_type,
                    value.data_type()
                )));
            }
            result.insert(output.name.clone(), value.to_json());
        }
        Ok(Value::Object(result).to_string())
    }
}

impl BrainTool for DreamNodeGraphTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(DreamNodeGraphFunctionTool {
            definition: self.definition.clone(),
        })
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.definition.run_duration
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.run_node_graph(call_content, arguments)
            .unwrap_or_else(|error| format!("Dream node graph tool '{}' failed: {error}", self.definition.name))
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
}

#[derive(Debug)]
struct DreamNodeGraphFunctionTool {
    definition: BrainToolDefinition,
}

impl FunctionTool for DreamNodeGraphFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> Value {
        tool_parameters_to_json_schema(&self.definition.parameters)
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}

fn tool_parameters_to_json_schema(parameters: &[ToolParamDef]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for parameter in parameters {
        if parameter.required {
            required.push(Value::String(parameter.name.clone()));
        }
        properties.insert(
            parameter.name.clone(),
            json!({
                "type": data_type_to_json_schema_type(&parameter.data_type),
                "description": parameter.desc,
            }),
        );
    }
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn data_type_to_json_schema_type(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::String | DataType::Password | DataType::Binary => "string",
        DataType::Integer => "integer",
        DataType::Float => "number",
        DataType::Boolean => "boolean",
        DataType::Vec(_) | DataType::Vector => "array",
        _ => "object",
    }
}

pub fn run_dream_agent(
    llm: Arc<dyn LLMBase>,
    previous_memory: &str,
    transcript: &str,
    tool_definitions: Vec<BrainToolDefinition>,
) -> Result<String> {
    let messages = vec![
        LLMMessage::system(DREAM_SYSTEM_PROMPT),
        LLMMessage::user(format!(
            "Combine the previous Dream memory with this conversation. Record durable facts, preferences, relationships, emotions, and emotional continuity. Do not invent information.\n\nPrevious Dream memory:\n{previous_memory}\n\nCurrent conversation:\n{transcript}"
        )),
    ];
    let mut brain = Brain::new(llm);
    for definition in tool_definitions.into_iter().filter(BrainToolDefinition::uses_subgraph) {
        brain.add_tool(DreamNodeGraphTool::new(definition));
    }
    let (output, stop_reason) = brain.run(messages);
    if !matches!(stop_reason, BrainStopReason::Done) {
        return Err(Error::StringError("Dream Agent did not complete normally".to_string()));
    }
    output
        .last()
        .and_then(LLMMessage::content_text_owned)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| Error::StringError("Dream Agent returned no text".to_string()))
}
