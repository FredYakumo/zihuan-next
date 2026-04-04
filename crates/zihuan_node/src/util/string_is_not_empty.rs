use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::StringIsNotEmptyNode;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    fn run(input: &str, trim: Option<bool>) -> bool {
        let mut node = StringIsNotEmptyNode::new("test", "Test");
        let mut map = HashMap::new();
        map.insert("input".to_string(), DataValue::String(input.to_string()));
        if let Some(t) = trim {
            map.insert("trim_before_check".to_string(), DataValue::Boolean(t));
        }
        match node.execute(map).unwrap().remove("result").unwrap() {
            DataValue::Boolean(b) => b,
            _ => panic!("expected Boolean"),
        }
    }

    #[test]
    fn non_empty_string_is_true() {
        assert!(run("hello", None));
    }

    #[test]
    fn empty_string_is_false() {
        assert!(!run("", None));
    }

    #[test]
    fn spaces_without_trim_is_true() {
        assert!(run("   ", None));
        assert!(run("   ", Some(false)));
    }

    #[test]
    fn spaces_with_trim_is_false() {
        assert!(!run("   ", Some(true)));
    }

    #[test]
    fn non_empty_with_trim_is_true() {
        assert!(run("  hi  ", Some(true)));
    }
}
