use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct StackNode {
    id: String,
    name: String,
}

impl StackNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StackNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将单个元素封装为单元素 List")
    }

    node_input![port! { name = "element", ty = Any, desc = "要封装到数组中的元素" },];

    node_output![port! { name = "array", ty = Vec(Any), desc = "包含单个元素的 List" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let element = inputs
            .get("element")
            .cloned()
            .ok_or_else(|| zihuan_core::error::Error::ValidationError("元素输入不存在".to_string()))?;

        let element_type = element.data_type();
        let mut outputs = HashMap::new();
        outputs.insert(
            "array".to_string(),
            DataValue::Vec(Box::new(element_type), vec![element]),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
