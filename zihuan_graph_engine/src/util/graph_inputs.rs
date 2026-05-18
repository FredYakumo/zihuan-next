use std::collections::HashMap;

use serde_json::Value;

use crate::function_graph::{
    FunctionPortDef, FUNCTION_RUNTIME_VALUES_PORT, FUNCTION_SIGNATURE_PORT,
};
use crate::graph_boundary::graph_inputs_ports;
use crate::util::function::data_value_from_json_with_declared_type;
use crate::{DataValue, Node, Port};
use zihuan_core::error::{Error, Result};

pub struct GraphInputsNode {
    id: String,
    name: String,
    signature: Vec<FunctionPortDef>,
    runtime_values: Option<HashMap<String, DataValue>>,
}

impl GraphInputsNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            signature: Vec::new(),
            runtime_values: None,
        }
    }

    fn apply_signature_json(&mut self, value: &Value) -> Result<()> {
        self.signature =
            serde_json::from_value::<Vec<FunctionPortDef>>(value.clone()).map_err(|_| {
                Error::ValidationError(
                    "graph_inputs.signature 不是有效的节点图签名 JSON".to_string(),
                )
            })?;
        Ok(())
    }
}

impl Node for GraphInputsNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("主节点图输入边界，将运行时参数展开成动态输出端口")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            crate::function_graph::hidden_function_signature_port(),
            crate::function_graph::hidden_function_runtime_values_port(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        graph_inputs_ports(&self.signature)
    }

    fn has_dynamic_output_ports(&self) -> bool {
        true
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(FUNCTION_SIGNATURE_PORT) {
            Some(DataValue::Json(value)) => self.apply_signature_json(value),
            Some(other) => Err(Error::ValidationError(format!(
                "graph_inputs.signature 需要 Json，实际为 {}",
                other.data_type()
            ))),
            None => Ok(()),
        }
    }

    fn set_function_runtime_values(&mut self, values: HashMap<String, DataValue>) -> Result<()> {
        self.runtime_values = Some(values);
        Ok(())
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        if let Some(DataValue::Json(value)) = inputs.get(FUNCTION_SIGNATURE_PORT) {
            self.apply_signature_json(value)?;
        }
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        for port in &self.signature {
            if let Some(runtime_values) = &self.runtime_values {
                let value = match runtime_values.get(&port.name) {
                    Some(value) => value,
                    None if !port.required => continue,
                    None => {
                        return Err(Error::ValidationError(format!(
                            "节点图输入 '{}' 在 runtime_values 中缺失",
                            port.name
                        )))
                    }
                };
                outputs.insert(port.name.clone(), value.clone());
                continue;
            }

            let runtime_values = match inputs.get(FUNCTION_RUNTIME_VALUES_PORT) {
                Some(DataValue::Json(Value::Object(map))) => map,
                Some(DataValue::Json(Value::Null)) | None => return Ok(HashMap::new()),
                Some(DataValue::Json(other)) => {
                    return Err(Error::ValidationError(format!(
                        "graph_inputs.runtime_values 需要 JSON 对象，实际为 {}",
                        other
                    )));
                }
                Some(other) => {
                    return Err(Error::ValidationError(format!(
                        "graph_inputs.runtime_values 需要 Json，实际为 {}",
                        other.data_type()
                    )));
                }
            };

            let value = match runtime_values.get(&port.name) {
                Some(value) => value,
                None if !port.required => continue,
                None => {
                    return Err(Error::ValidationError(format!(
                        "节点图输入 '{}' 在 runtime_values 中缺失",
                        port.name
                    )))
                }
            };
            outputs.insert(
                port.name.clone(),
                data_value_from_json_with_declared_type(port, value)?,
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::function_graph::FUNCTION_SIGNATURE_PORT;
    use crate::{DataType, DataValue, Node};

    use super::GraphInputsNode;

    fn test_signature() -> Vec<crate::function_graph::FunctionPortDef> {
        vec![
            crate::function_graph::FunctionPortDef {
                name: "required_text".to_string(),
                data_type: DataType::String,
                description: String::new(),
                required: true,
            },
            crate::function_graph::FunctionPortDef {
                name: "optional_text".to_string(),
                data_type: DataType::String,
                description: String::new(),
                required: false,
            },
        ]
    }

    fn build_node() -> GraphInputsNode {
        let mut node = GraphInputsNode::new("test", "test");
        node.apply_inline_config(&HashMap::from([(
            FUNCTION_SIGNATURE_PORT.to_string(),
            DataValue::Json(json!(test_signature())),
        )]))
        .expect("signature should apply");
        node
    }

    #[test]
    fn graph_inputs_skip_missing_optional_runtime_value() {
        let mut node = build_node();
        node.set_function_runtime_values(HashMap::from([(
            "required_text".to_string(),
            DataValue::String("hello".to_string()),
        )]))
        .expect("runtime values should apply");

        let outputs = node
            .execute(HashMap::new())
            .expect("execute should succeed");

        match outputs.get("required_text") {
            Some(DataValue::String(value)) => assert_eq!(value, "hello"),
            other => panic!("unexpected output: {other:?}"),
        }
        assert!(!outputs.contains_key("optional_text"));
    }

    #[test]
    fn graph_inputs_still_require_required_runtime_value() {
        let mut node = build_node();
        node.set_function_runtime_values(HashMap::new())
            .expect("runtime values should apply");

        let error = node
            .execute(HashMap::new())
            .expect_err("execute should fail");
        assert!(error.to_string().contains("required_text"));
    }

    #[test]
    fn graph_inputs_json_runtime_values_skip_missing_optional() {
        let mut node = build_node();
        let outputs = node
            .execute(HashMap::from([(
                crate::function_graph::FUNCTION_RUNTIME_VALUES_PORT.to_string(),
                DataValue::Json(json!({
                    "required_text": "hello"
                })),
            )]))
            .expect("execute should succeed");

        match outputs.get("required_text") {
            Some(DataValue::String(value)) => assert_eq!(value, "hello"),
            other => panic!("unexpected output: {other:?}"),
        }
        assert!(!outputs.contains_key("optional_text"));
    }
}
