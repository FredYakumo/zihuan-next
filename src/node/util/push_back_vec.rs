use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct PushBackVecNode {
    id: String,
    name: String,
}

impl PushBackVecNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for PushBackVecNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将单个元素追加到列表末尾，要求元素类型与列表元素类型一致")
    }

    node_input![
        port! { name = "vec", ty = Vec(Any), desc = "输入列表" },
        port! { name = "element", ty = Any, desc = "要追加到列表末尾的元素" },
    ];

    node_output![
        port! { name = "result", ty = Any, desc = "追加元素后的新列表，元素类型与输入列表一致" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let (vec_type, vec_items) = match inputs.get("vec") {
            Some(DataValue::Vec(inner_type, items)) => ((**inner_type).clone(), items),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "vec 输入必须为 Vec 类型".to_string(),
                ))
            }
        };

        let element = inputs
            .get("element")
            .cloned()
            .ok_or_else(|| crate::error::Error::ValidationError("element 输入不存在".to_string()))?;

        let element_type = element.data_type();
        if vec_type != element_type {
            return Err(crate::error::Error::ValidationError(format!(
                "vec 与 element 的元素类型不一致：vec 为 {}，element 为 {}",
                vec_type, element_type
            )));
        }

        let mut merged = Vec::with_capacity(vec_items.len() + 1);
        merged.extend(vec_items.iter().cloned());
        merged.push(element);

        let mut outputs = HashMap::new();
        outputs.insert(
            "result".to_string(),
            DataValue::Vec(Box::new(vec_type), merged),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::PushBackVecNode;
    use crate::error::Result;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn appends_element_to_vector_with_same_inner_type() -> Result<()> {
        let mut node = PushBackVecNode::new("push_back", "PushBack");
        let inputs = HashMap::from([
            (
                "vec".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    vec![DataValue::String("a".to_string())],
                ),
            ),
            (
                "element".to_string(),
                DataValue::String("b".to_string()),
            ),
        ]);

        let outputs = node.execute(inputs)?;
        match outputs.get("result") {
            Some(DataValue::Vec(inner, items)) => {
                assert_eq!(**inner, DataType::String);
                assert_eq!(items.len(), 2);
                assert!(matches!(&items[0], DataValue::String(value) if value == "a"));
                assert!(matches!(&items[1], DataValue::String(value) if value == "b"));
            }
            other => panic!("unexpected output: {:?}", other),
        }

        Ok(())
    }

    #[test]
    fn rejects_element_with_different_inner_type() {
        let mut node = PushBackVecNode::new("push_back", "PushBack");
        let inputs = HashMap::from([
            (
                "vec".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    vec![DataValue::String("a".to_string())],
                ),
            ),
            (
                "element".to_string(),
                DataValue::Integer(1),
            ),
        ]);

        let error = node.execute(inputs).expect_err("should reject mismatched element type");
        assert!(error
            .to_string()
            .contains("vec 与 element 的元素类型不一致"));
    }
}
