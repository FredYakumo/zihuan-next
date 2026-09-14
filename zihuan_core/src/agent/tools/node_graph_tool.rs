use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::agent::tools::{Tool, ToolExecutionOutput, ToolRunDuration};
use crate::graph::function_graph::{
    sync_function_subgraph_signature, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use crate::graph::graph_io::refresh_port_types;
use crate::graph::registry::build_node_graph_from_definition;
use crate::graph::tool_spec::{tool_calling_tool_input_signature, ToolDefinition, ToolParamDef};
use crate::graph::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use crate::graph::{DataType, DataValue};
use crate::model_inference::llm::tooling::FunctionTool;

/// Exposes a node-graph [`ToolDefinition`] as a callable tool.
///
/// The graph's `function_inputs` boundary is fed the declared parameters plus the assistant
/// `call_content`; the `function_outputs` boundary result is mapped back onto the declared
/// outputs and serialized as a JSON object.
pub struct NodeGraphTool {
    definition: ToolDefinition,
}

impl NodeGraphTool {
    pub fn new(definition: ToolDefinition) -> Self {
        Self { definition }
    }

    fn run_node_graph(
        &self,
        call_content: &str,
        arguments: &Value,
    ) -> crate::error::Result<String> {
        let arguments = arguments.as_object().ok_or_else(|| {
            crate::error::Error::ValidationError(format!(
                "Node graph tool '{}' requires JSON object arguments",
                self.definition.name
            ))
        })?;
        let mut runtime_values = HashMap::new();
        runtime_values.insert("content".to_string(), DataValue::String(call_content.to_string()));
        for parameter in &self.definition.parameters {
            let Some(value) = arguments.get(&parameter.name) else {
                if parameter.required {
                    return Err(crate::error::Error::ValidationError(format!(
                        "Node graph tool '{}' is missing required parameter '{}'",
                        self.definition.name, parameter.name
                    )));
                }
                continue;
            };
            if value.is_null() && !parameter.required {
                continue;
            }
            let port = crate::graph::function_graph::FunctionPortDef {
                name: parameter.name.clone(),
                data_type: parameter.data_type.clone(),
                description: parameter.desc.clone(),
                required: parameter.required,
            };
            runtime_values.insert(
                parameter.name.clone(),
                data_value_from_json_with_declared_type(&port, value)?,
            );
        }
        let input_signature =
            tool_calling_tool_input_signature("tool_calling", &[], &self.definition);
        let mut subgraph = self.definition.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &self.definition.outputs);
        refresh_port_types(&mut subgraph);
        let inputs = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' is missing the function_inputs boundary node",
                    self.definition.name
                ))
            })?;
        inputs.inline_values.insert(
            crate::graph::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );
        let outputs = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' is missing the function_outputs boundary node",
                    self.definition.name
                ))
            })?;
        outputs.inline_values.insert(
            crate::graph::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&self.definition.outputs).unwrap_or(Value::Null),
        );
        let mut graph = build_node_graph_from_definition(&subgraph).map_err(|error| {
            crate::error::Error::ValidationError(format!(
                "Node graph tool '{}' could not build its subgraph: {error}",
                self.definition.name
            ))
        })?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values.into())
            .map_err(|error| {
                crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' could not inject runtime inputs: {error}",
                    self.definition.name
                ))
            })?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(error) = execution_result.error_message {
            return Err(crate::error::Error::ValidationError(format!(
                "Node graph tool '{}' failed: {error}",
                self.definition.name
            )));
        }
        let output_values =
            execution_result.node_results.get(FUNCTION_OUTPUTS_NODE_ID).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' produced no function_outputs result",
                    self.definition.name
                ))
            })?;
        let mut result = Map::new();
        for output in &self.definition.outputs {
            let value = output_values.get(&output.name).ok_or_else(|| {
                crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' did not provide output '{}'",
                    self.definition.name, output.name
                ))
            })?;
            if !output.data_type.is_compatible_with(&value.data_type()) {
                return Err(crate::error::Error::ValidationError(format!(
                    "Node graph tool '{}' output '{}' type mismatch: expected {}, got {}",
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

impl Tool for NodeGraphTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(NodeGraphFunctionTool { definition: self.definition.clone() })
    }
    fn run_duration(&self) -> ToolRunDuration {
        self.definition.run_duration
    }
    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.run_node_graph(call_content, arguments).unwrap_or_else(|error| {
            format!("Node graph tool '{}' failed: {error}", self.definition.name)
        })
    }
    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
}

#[derive(Debug)]
struct NodeGraphFunctionTool {
    definition: ToolDefinition,
}

impl FunctionTool for NodeGraphFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }
    fn description(&self) -> &str {
        &self.definition.description
    }
    fn parameters(&self) -> Value {
        tool_parameters_to_json_schema(&self.definition.parameters)
    }
    fn call(&self, arguments: Value) -> crate::error::Result<Value> {
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
    json!({"type": "object", "properties": properties, "required": required})
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
