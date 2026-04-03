use std::collections::HashMap;

use serde_json::Value;

use zihuan_core::error::{Error, Result};
use crate::function_graph::{
    embedded_function_config_from_value, function_inputs_ports, function_outputs_ports,
    hidden_function_config_port, sync_function_subgraph_signature, EmbeddedFunctionConfig,
    FunctionPortDef, FUNCTION_CONFIG_PORT, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use crate::graph_io::refresh_port_types;
use crate::registry::{build_node_graph_from_definition, json_to_data_value, NODE_REGISTRY};
use crate::{DataValue, Node, Port};

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Once};

    use super::FunctionNode;
    use zihuan_llm_types::llm_base::LLMBase;
    use zihuan_llm_types::{InferenceParam, OpenAIMessage};
    use crate::function_graph::{
        default_embedded_function_config, sync_function_subgraph_signature, FunctionPortDef,
        FUNCTION_CONFIG_PORT, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
    };
    use crate::graph_io::EdgeDefinition;
    use crate::{DataType, DataValue, Node};

    fn ensure_registry_initialized() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            crate::registry::init_node_registry().expect("registry should initialize");
        });
    }

    fn echo_config() -> crate::function_graph::EmbeddedFunctionConfig {
        let mut config = default_embedded_function_config("Echo");
        config.inputs = vec![FunctionPortDef {
            name: "text".to_string(),
            data_type: DataType::String,
        }];
        config.outputs = vec![FunctionPortDef {
            name: "result".to_string(),
            data_type: DataType::String,
        }];
        sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);
        config.subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "text".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "result".to_string(),
        });
        config
    }

    #[derive(Debug)]
    struct StubLlm;

    impl LLMBase for StubLlm {
        fn get_model_name(&self) -> &str {
            "stub-llm"
        }

        fn inference(&self, _param: &InferenceParam) -> OpenAIMessage {
            OpenAIMessage::system("unused")
        }
    }

    #[test]
    fn execute_runs_embedded_subgraph_and_collects_declared_outputs() {
        ensure_registry_initialized();

        let config = echo_config();
        let mut node = FunctionNode::new("outer_fn", "Echo");
        node.apply_inline_config(&HashMap::from([(
            FUNCTION_CONFIG_PORT.to_string(),
            DataValue::Json(serde_json::to_value(&config).unwrap()),
        )]))
        .expect("function config should apply");

        let outputs = node
            .execute(HashMap::from([(
                "text".to_string(),
                DataValue::String("hello".to_string()),
            )]))
            .expect("function should execute");

        match outputs.get("result") {
            Some(DataValue::String(value)) => assert_eq!(value, "hello"),
            other => panic!("unexpected function output: {other:?}"),
        }
    }

    #[test]
    fn execute_wraps_subgraph_output_errors_with_outer_node_id() {
        ensure_registry_initialized();

        let mut config = default_embedded_function_config("Broken");
        config.inputs = vec![FunctionPortDef {
            name: "text".to_string(),
            data_type: DataType::String,
        }];
        config.outputs = vec![FunctionPortDef {
            name: "result".to_string(),
            data_type: DataType::String,
        }];
        sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);

        let mut node = FunctionNode::new("outer_fn", "Broken");
        node.apply_inline_config(&HashMap::from([(
            FUNCTION_CONFIG_PORT.to_string(),
            DataValue::Json(serde_json::to_value(&config).unwrap()),
        )]))
        .expect("function config should apply");

        let error = node
            .execute(HashMap::from([(
                "text".to_string(),
                DataValue::String("hello".to_string()),
            )]))
            .expect_err("missing output edge should fail");

        let error_text = error.to_string();
        assert!(error_text.contains("[NODE_ERROR:outer_fn]"));
        assert!(error_text.contains("函数子图执行失败"));
    }

    #[test]
    fn execute_preserves_runtime_reference_inputs_in_subgraph() {
        ensure_registry_initialized();

        let mut config = default_embedded_function_config("RefEcho");
        config.inputs = vec![FunctionPortDef {
            name: "llm_ref".to_string(),
            data_type: DataType::LLModel,
        }];
        config.outputs = vec![FunctionPortDef {
            name: "llm_ref".to_string(),
            data_type: DataType::LLModel,
        }];
        sync_function_subgraph_signature(&mut config.subgraph, &config.inputs, &config.outputs);
        config.subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "llm_ref".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "llm_ref".to_string(),
        });

        let mut node = FunctionNode::new("outer_fn", "RefEcho");
        node.apply_inline_config(&HashMap::from([(
            FUNCTION_CONFIG_PORT.to_string(),
            DataValue::Json(serde_json::to_value(&config).unwrap()),
        )]))
        .expect("function config should apply");

        let llm_ref = Arc::new(StubLlm) as Arc<dyn LLMBase>;
        let outputs = node
            .execute(HashMap::from([(
                "llm_ref".to_string(),
                DataValue::LLModel(llm_ref.clone()),
            )]))
            .expect("function should preserve runtime references");

        match outputs.get("llm_ref") {
            Some(DataValue::LLModel(output_ref)) => {
                assert_eq!(output_ref.get_model_name(), llm_ref.get_model_name());
            }
            other => panic!("unexpected function output: {other:?}"),
        }
    }
}
