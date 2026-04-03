use zihuan_core::error::{Error, Result};
use crate::{DataType, DataValue, Node, Port, RuntimeVariableStore};
use std::collections::HashMap;

pub const SET_VARIABLE_NAME_PORT: &str = "variable_name";
pub const SET_VARIABLE_TYPE_PORT: &str = "variable_type";
pub const SET_VARIABLE_VALUE_PORT: &str = "value";

#[derive(Debug, Clone)]
pub struct SetVariableNode {
    id: String,
    name: String,
    variable_name: Option<String>,
    variable_type: Option<DataType>,
    runtime_variable_store: Option<RuntimeVariableStore>,
}

impl SetVariableNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            variable_name: None,
            variable_type: None,
            runtime_variable_store: None,
        }
    }
}

impl Node for SetVariableNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将输入值写入运行期节点图变量，变量值会在每次重新运行时重置为初始值")
    }

    fn has_dynamic_input_ports(&self) -> bool {
        true
    }

    fn input_ports(&self) -> Vec<Port> {
        let mut ports = vec![
            Port::new(SET_VARIABLE_NAME_PORT, DataType::String)
                .with_description("要写入的变量名，由 UI 选择")
                .optional(),
            Port::new(SET_VARIABLE_TYPE_PORT, DataType::String)
                .with_description("所选变量类型，由 UI 维护")
                .optional(),
        ];

        if let Some(data_type) = &self.variable_type {
            ports.push(
                Port::new(SET_VARIABLE_VALUE_PORT, data_type.clone())
                    .with_description("写入变量的新值"),
            );
        }

        ports
    }

    fn output_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.variable_name =
            inline_values
                .get(SET_VARIABLE_NAME_PORT)
                .and_then(|value| match value {
                    DataValue::String(value) if !value.trim().is_empty() => {
                        Some(value.trim().to_string())
                    }
                    _ => None,
                });

        self.variable_type =
            inline_values
                .get(SET_VARIABLE_TYPE_PORT)
                .and_then(|value| match value {
                    DataValue::String(value) => match value.as_str() {
                        "Integer" => Some(DataType::Integer),
                        "Float" => Some(DataType::Float),
                        "Boolean" => Some(DataType::Boolean),
                        "Password" => Some(DataType::Password),
                        "String" => Some(DataType::String),
                        _ => None,
                    },
                    _ => None,
                });

        Ok(())
    }

    fn set_runtime_variable_store(&mut self, store: RuntimeVariableStore) {
        self.runtime_variable_store = Some(store);
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let variable_name = self
            .variable_name
            .clone()
            .ok_or_else(|| Error::ValidationError("未选择变量".to_string()))?;
        let value = inputs
            .get(SET_VARIABLE_VALUE_PORT)
            .cloned()
            .ok_or_else(|| Error::InvalidNodeInput("value is required".to_string()))?;
        let store = self
            .runtime_variable_store
            .as_ref()
            .ok_or_else(|| Error::ValidationError("运行期变量存储未初始化".to_string()))?;

        store.write().unwrap().insert(variable_name, value);
        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{SetVariableNode, SET_VARIABLE_NAME_PORT, SET_VARIABLE_TYPE_PORT, SET_VARIABLE_VALUE_PORT};
    use crate::{DataType, DataValue, Node, RuntimeVariableStore};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    #[test]
    fn execute_writes_selected_variable_into_runtime_store() {
        let mut node = SetVariableNode::new("set_var", "Set Var");
        node.apply_inline_config(&HashMap::from([
            (
                SET_VARIABLE_NAME_PORT.to_string(),
                DataValue::String("answer".to_string()),
            ),
            (
                SET_VARIABLE_TYPE_PORT.to_string(),
                DataValue::String("String".to_string()),
            ),
        ]))
        .unwrap();

        let store: RuntimeVariableStore = Arc::new(RwLock::new(HashMap::new()));
        node.set_runtime_variable_store(store.clone());
        node.execute(HashMap::from([(
            SET_VARIABLE_VALUE_PORT.to_string(),
            DataValue::String("42".to_string()),
        )]))
        .unwrap();

        let value = store.read().unwrap().get("answer").cloned();
        assert!(matches!(value, Some(DataValue::String(value)) if value == "42"));
        assert_eq!(node.input_ports().last().map(|port| port.data_type.clone()), Some(DataType::String));
    }
}
