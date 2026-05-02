use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct StringIsNotEmptyNode {
    id: String,
    name: String,
}

impl StringIsNotEmptyNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StringIsNotEmptyNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("判断字符串是否非空，可选 trim_before_check 决定是否在判断前先去除两端空白")
    }

    node_input![
        port! { name = "input", ty = String, desc = "待判断的字符串" },
        port! { name = "trim_before_check", ty = Boolean, optional, desc = "为 true 时先 trim 再判断，纯空格字符串将视为空；默认 false" },
    ];

    node_output![
        port! { name = "result", ty = Boolean, desc = "字符串非空则为 true，否则为 false" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let input = match inputs.get("input") {
            Some(DataValue::String(s)) => s.clone(),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "input 输入必须为 String 类型".to_string(),
                ))
            }
        };

        let trim = match inputs.get("trim_before_check") {
            Some(DataValue::Boolean(b)) => *b,
            _ => false,
        };

        let is_not_empty = if trim {
            !input.trim().is_empty()
        } else {
            !input.is_empty()
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::Boolean(is_not_empty));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

