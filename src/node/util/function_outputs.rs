use std::collections::HashMap;

use serde_json::Value;

use crate::error::{Error, Result};
use crate::node::function_graph::{
    function_outputs_ports, function_signature_from_value, hidden_function_signature_port,
    FunctionPortDef, FUNCTION_SIGNATURE_PORT,
};
use crate::node::{DataValue, Node, Port};

pub struct FunctionOutputsNode {
    id: String,
    name: String,
    signature: Vec<FunctionPortDef>,
}

impl FunctionOutputsNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            signature: Vec::new(),
        }
    }

    fn apply_signature_json(&mut self, value: &Value) -> Result<()> {
        self.signature = function_signature_from_value(value).ok_or_else(|| {
            Error::ValidationError(
                "function_outputs.signature 不是有效的函数签名 JSON".to_string(),
            )
        })?;
        Ok(())
    }
}

impl Node for FunctionOutputsNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("函数子图输出边界，将声明的输出端口聚合为结果")
    }

    fn input_ports(&self) -> Vec<Port> {
        let mut ports = vec![hidden_function_signature_port()];
        ports.extend(function_outputs_ports(&self.signature));
        ports
    }

    fn output_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn has_dynamic_input_ports(&self) -> bool {
        true
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(FUNCTION_SIGNATURE_PORT) {
            Some(DataValue::Json(value)) => self.apply_signature_json(value),
            Some(other) => Err(Error::ValidationError(format!(
                "function_outputs.signature 需要 Json，实际为 {}",
                other.data_type()
            ))),
            None => Ok(()),
        }
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        if let Some(DataValue::Json(value)) = inputs.get(FUNCTION_SIGNATURE_PORT) {
            self.apply_signature_json(value)?;
        }
        self.validate_inputs(&inputs)?;
        Ok(HashMap::new())
    }
}
