use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct JoinStringNode {
    id: String,
    name: String,
}

impl JoinStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for JoinStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用分隔符将 Vec<String> 拼接为单个字符串")
    }

    node_input![
        port! { name = "strings", ty = Vec(String), desc = "要拼接的字符串列表" },
        port! { name = "delimiter", ty = String, desc = "字符串之间使用的分隔符" },
    ];

    node_output![port! { name = "result", ty = String, desc = "拼接后的字符串" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let strings = match inputs.get("strings") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::String => items,
            Some(DataValue::Vec(inner_type, _)) => {
                return Err(zihuan_core::error::Error::ValidationError(format!(
                    "strings 输入必须为 Vec<String>，实际为 Vec<{}>",
                    inner_type
                )))
            }
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "strings 输入必须为 Vec<String>".to_string(),
                ))
            }
        };

        let delimiter = match inputs.get("delimiter") {
            Some(DataValue::String(value)) => value,
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "delimiter 输入必须为 String 类型".to_string(),
                ))
            }
        };

        let joined = strings
            .iter()
            .map(|value| match value {
                DataValue::String(text) => Ok(text.as_str()),
                other => Err(zihuan_core::error::Error::ValidationError(format!(
                    "strings 中包含非字符串元素：{}",
                    other.data_type()
                ))),
            })
            .collect::<Result<Vec<_>>>()?
            .join(delimiter);

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::String(joined));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
