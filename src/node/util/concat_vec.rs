use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

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
        port! { name = "vec", ty = Any, desc = "拼接后的列表，元素类型与输入列表一致" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let (vec1_type, vec1_items) = match inputs.get("vec1") {
            Some(DataValue::Vec(inner_type, items)) => ((**inner_type).clone(), items),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "vec1 输入必须为 Vec 类型".to_string(),
                ))
            }
        };

        let (vec2_type, vec2_items) = match inputs.get("vec2") {
            Some(DataValue::Vec(inner_type, items)) => ((**inner_type).clone(), items),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "vec2 输入必须为 Vec 类型".to_string(),
                ))
            }
        };

        if vec1_type != vec2_type {
            return Err(crate::error::Error::ValidationError(format!(
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

#[cfg(test)]
mod tests {
    use super::ConcatVecNode;
    use crate::error::Result;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn concatenates_vectors_with_same_inner_type() -> Result<()> {
        let mut node = ConcatVecNode::new("concat", "Concat");
        let inputs = HashMap::from([
            (
                "vec1".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    vec![DataValue::String("a".to_string())],
                ),
            ),
            (
                "vec2".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    vec![DataValue::String("b".to_string())],
                ),
            ),
        ]);

        let outputs = node.execute(inputs)?;
        match outputs.get("vec") {
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
    fn rejects_vectors_with_different_inner_types() {
        let mut node = ConcatVecNode::new("concat", "Concat");
        let inputs = HashMap::from([
            (
                "vec1".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    vec![DataValue::String("a".to_string())],
                ),
            ),
            (
                "vec2".to_string(),
                DataValue::Vec(
                    Box::new(DataType::Integer),
                    vec![DataValue::Integer(1)],
                ),
            ),
        ]);

        let error = node.execute(inputs).expect_err("should reject mismatched vector types");
        assert!(error
            .to_string()
            .contains("vec1 与 vec2 的元素类型不一致"));
    }
}