use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct ConcatVecNode {
    id: String,
    name: String,
}

impl ConcatVecNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConcatVecNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 vec2 拼接到 vec1 之后，要求两个 Vec 的元素类型一致")
    }

    node_input![
        port! { name = "vec1", ty = Vec(Any), desc = "前半部分列表" },
        port! { name = "vec2", ty = Vec(Any), desc = "后半部分列表，将拼接到 vec1 后面" },
    ];

    node_output![
        port! { name = "vec", ty = Vec(Any), desc = "拼接后的列表，元素类型与输入列表一致" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let (vec1_type, vec1_items) = match inputs.get("vec1") {
            Some(DataValue::Vec(inner_type, items)) => ((**inner_type).clone(), items),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "vec1 输入必须为 Vec 类型".to_string(),
                ))
            }
        };

        let (vec2_type, vec2_items) = match inputs.get("vec2") {
            Some(DataValue::Vec(inner_type, items)) => ((**inner_type).clone(), items),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "vec2 输入必须为 Vec 类型".to_string(),
                ))
            }
        };

        if !vec1_type.is_compatible_with(&vec2_type) {
            return Err(zihuan_core::error::Error::ValidationError(format!(
                "vec1 与 vec2 的元素类型不一致：vec1 为 {}，vec2 为 {}",
                vec1_type, vec2_type
            )));
        }

        let mut merged = Vec::with_capacity(vec1_items.len() + vec2_items.len());
        merged.extend(vec1_items.iter().cloned());
        merged.extend(vec2_items.iter().cloned());

        let mut outputs = HashMap::new();
        outputs.insert(
            "vec".to_string(),
            DataValue::Vec(Box::new(vec1_type), merged),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
