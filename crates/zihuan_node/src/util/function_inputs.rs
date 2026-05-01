use std::collections::HashMap;

use serde_json::Value;

use crate::function_graph::{
    function_inputs_ports, function_signature_from_value, hidden_function_runtime_values_port,
    hidden_function_signature_port, FunctionPortDef, FUNCTION_RUNTIME_VALUES_PORT,
    FUNCTION_SIGNATURE_PORT,
};
use crate::util::function::data_value_from_json_with_declared_type;
use crate::{DataValue, Node, Port};
use zihuan_core::error::{Error, Result};

pub struct FunctionInputsNode {
    id: String,
    name: String,
    signature: Vec<FunctionPortDef>,
    runtime_values: Option<HashMap<String, DataValue>>,
}

impl FunctionInputsNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            signature: Vec::new(),
            runtime_values: None,
        }
    }

    fn apply_signature_json(&mut self, value: &Value) -> Result<()> {
        self.signature = function_signature_from_value(value).ok_or_else(|| {
            Error::ValidationError("function_inputs.signature 不是有效的函数签名 JSON".to_string())
        })?;
        Ok(())
    }
}

impl Node for FunctionInputsNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("函数子图输入边界，将运行时参数展开成动态输出端口")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            hidden_function_signature_port(),
            hidden_function_runtime_values_port(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        function_inputs_ports(&self.signature)
    }

    fn has_dynamic_output_ports(&self) -> bool {
        true
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(FUNCTION_SIGNATURE_PORT) {
            Some(DataValue::Json(value)) => self.apply_signature_json(value),
            Some(other) => Err(Error::ValidationError(format!(
                "function_inputs.signature 需要 Json，实际为 {}",
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
                let value = runtime_values.get(&port.name).ok_or_else(|| {
                    Error::ValidationError(format!(
                        "函数输入 '{}' 在 runtime_values 中缺失",
                        port.name
                    ))
                })?;
                outputs.insert(port.name.clone(), value.clone());
                continue;
            }

            let runtime_values = match inputs.get(FUNCTION_RUNTIME_VALUES_PORT) {
                Some(DataValue::Json(Value::Object(map))) => map,
                Some(DataValue::Json(Value::Null)) | None => {
                    return Ok(HashMap::new());
                }
                Some(DataValue::Json(other)) => {
                    return Err(Error::ValidationError(format!(
                        "function_inputs.runtime_values 需要 JSON 对象，实际为 {}",
                        other
                    )));
                }
                Some(other) => {
                    return Err(Error::ValidationError(format!(
                        "function_inputs.runtime_values 需要 Json，实际为 {}",
                        other.data_type()
                    )));
                }
            };

            let value = runtime_values.get(&port.name).ok_or_else(|| {
                Error::ValidationError(format!("函数输入 '{}' 在 runtime_values 中缺失", port.name))
            })?;
            outputs.insert(
                port.name.clone(),
                data_value_from_json_with_declared_type(port, value)?,
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
