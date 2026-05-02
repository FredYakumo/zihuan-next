use std::collections::HashMap;

use serde_json::Value;

use crate::function_graph::{
    embedded_function_config_from_value, function_inputs_ports, function_outputs_ports,
    hidden_function_config_port, sync_function_subgraph_signature, EmbeddedFunctionConfig,
    FunctionPortDef, FUNCTION_CONFIG_PORT, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use crate::graph_io::refresh_port_types;
use crate::registry::{build_node_graph_from_definition, json_to_data_value, NODE_REGISTRY};
use crate::{DataValue, Node, Port};
use zihuan_core::error::{Error, Result};

pub struct FunctionNode {
    id: String,
    name: String,
    config: EmbeddedFunctionConfig,
}

impl FunctionNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: id.into(),
            name: name.clone(),
            config: crate::function_graph::default_embedded_function_config(name),
        }
    }

    fn set_config(&mut self, mut config: EmbeddedFunctionConfig) -> Result<()> {
        if config.name.trim().is_empty() {
            config.name = self.name.clone();
        }
        sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);
        self.name = config.name.clone();
        self.config = config;
        Ok(())
    }

    fn wrap_error(&self, message: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, message.into()))
    }

    fn parse_config_input(&mut self, inputs: &HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::Json(value)) = inputs.get(FUNCTION_CONFIG_PORT) {
            let config = embedded_function_config_from_value(value)
                .ok_or_else(|| self.wrap_error("function_config 不是有效的函数配置 JSON"))?;
            self.set_config(config)?;
        }
        Ok(())
    }

    fn runtime_values_from_inputs(
        &self,
        inputs: &HashMap<String, DataValue>,
    ) -> HashMap<String, DataValue> {
        self.config
            .inputs
            .iter()
            .filter_map(|port| {
                inputs
                    .get(&port.name)
                    .map(|value| (port.name.clone(), value.clone()))
            })
            .collect()
    }

    fn ensure_subgraph_is_runnable(&self) -> Result<()> {
        for node in &self.config.subgraph.nodes {
            if NODE_REGISTRY.is_event_producer(&node.node_type) {
                return Err(self.wrap_error(format!(
                    "函数子图内不允许事件源节点：{} ({})",
                    node.name, node.node_type
                )));
            }
        }
        Ok(())
    }

    fn collect_declared_outputs(
        &self,
        node_results: &HashMap<String, HashMap<String, DataValue>>,
    ) -> Result<HashMap<String, DataValue>> {
        // No declared outputs → nothing to collect, skip boundary node lookup entirely.
        if self.config.outputs.is_empty() {
            return Ok(HashMap::new());
        }

        let Some(result_node_values) = node_results.get(FUNCTION_OUTPUTS_NODE_ID) else {
            return Err(self.wrap_error("函数子图缺少 function_outputs 边界节点执行结果"));
        };

        let mut outputs = HashMap::new();
        for port in &self.config.outputs {
            let value = result_node_values.get(&port.name).ok_or_else(|| {
                self.wrap_error(format!("函数输出 '{}' 未在子图中提供", port.name))
            })?;
            if !port.data_type.is_compatible_with(&value.data_type()) {
                return Err(self.wrap_error(format!(
                    "函数输出 '{}' 类型不匹配：声明为 {}，实际为 {}",
                    port.name,
                    port.data_type,
                    value.data_type()
                )));
            }
            outputs.insert(port.name.clone(), value.clone());
        }

        Ok(outputs)
    }
}

impl Node for FunctionNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        if self.config.description.trim().is_empty() {
            Some("执行节点私有的函数子图")
        } else {
            Some(self.config.description.as_str())
        }
    }

    fn input_ports(&self) -> Vec<Port> {
        let mut ports = vec![hidden_function_config_port()];
        ports.extend(function_inputs_ports(&self.config.inputs));
        ports
    }

    fn output_ports(&self) -> Vec<Port> {
        function_outputs_ports(&self.config.outputs)
    }

    fn has_dynamic_input_ports(&self) -> bool {
        true
    }

    fn has_dynamic_output_ports(&self) -> bool {
        true
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(FUNCTION_CONFIG_PORT) {
            Some(DataValue::Json(value)) => {
                let config = embedded_function_config_from_value(value)
                    .ok_or_else(|| self.wrap_error("function_config 不是有效的函数配置 JSON"))?;
                self.set_config(config)
            }
            Some(other) => Err(self.wrap_error(format!(
                "function_config 需要 Json，实际为 {}",
                other.data_type()
            ))),
            None => Ok(()),
        }
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.parse_config_input(&inputs)?;
        self.validate_inputs(&inputs)?;
        self.ensure_subgraph_is_runnable()?;

        let runtime_values = self.runtime_values_from_inputs(&inputs);
        let mut subgraph = self.config.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &self.config.inputs, &self.config.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error("函数子图缺少 function_inputs 边界节点"))?;
        function_inputs_node.inline_values.insert(
            crate::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&self.config.inputs).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error("函数子图缺少 function_outputs 边界节点"))?;
        function_outputs_node.inline_values.insert(
            crate::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&self.config.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph)
            .map_err(|e| self.wrap_error(format!("构建函数子图失败: {e}")))?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values)
            .map_err(|e| self.wrap_error(format!("注入函数运行时输入失败: {e}")))?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(error_message) = execution_result.error_message {
            return Err(self.wrap_error(format!("函数子图执行失败: {error_message}")));
        }

        let outputs = self.collect_declared_outputs(&execution_result.node_results)?;
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

pub fn data_value_from_json_with_declared_type(
    port: &FunctionPortDef,
    value: &Value,
) -> Result<DataValue> {
    json_to_data_value(value, &port.data_type).ok_or_else(|| {
        Error::ValidationError(format!(
            "端口 '{}' 期望类型 {}，但无法从 JSON 值 {} 解析",
            port.name, port.data_type, value
        ))
    })
}

pub fn inject_runtime_values_into_function_inputs_node(
    graph: &mut crate::NodeGraph,
    runtime_values: HashMap<String, DataValue>,
) -> Result<()> {
    let function_inputs_node = graph
        .nodes
        .get_mut(FUNCTION_INPUTS_NODE_ID)
        .ok_or_else(|| {
            Error::ValidationError("函数子图缺少 function_inputs 边界节点".to_string())
        })?;
    function_inputs_node.set_function_runtime_values(runtime_values)
}
