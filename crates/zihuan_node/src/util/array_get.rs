use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct ArrayGetNode {
    id: String,
    name: String,
}

impl ArrayGetNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ArrayGetNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从列表中按下标取元素，支持负数下标（-1为最后一个）")
    }

    node_input![
        port! { name = "array", ty = Vec(Any), desc = "输入列表" },
        port! { name = "index", ty = Integer, desc = "元素下标，负数表示从末尾倒数（-1为最后一个）" },
    ];

    node_output![
        port! { name = "element", ty = Any, desc = "提取出的元素，类型与列表中元素的类型一致" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let list = match inputs.get("array") {
            Some(DataValue::Vec(_, items)) => items,
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "array 输入必须为 List 类型".to_string(),
                ))
            }
        };

        let index = match inputs.get("index") {
            Some(DataValue::Integer(i)) => *i,
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "index 输入必须为 Integer 类型".to_string(),
                ))
            }
        };

        let len = list.len() as i64;
        let actual = if index < 0 { len + index } else { index };

        if actual < 0 || actual >= len {
            return Err(zihuan_core::error::Error::ValidationError(format!(
                "下标 {} 超出列表范围（长度 {}）",
                index, len
            )));
        }

        let element = list[actual as usize].clone();
        let mut outputs = HashMap::new();
        outputs.insert("element".to_string(), element);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
