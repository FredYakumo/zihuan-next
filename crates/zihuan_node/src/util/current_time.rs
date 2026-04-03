use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use chrono::Local;
use std::collections::HashMap;

pub struct CurrentTimeNode {
    id: String,
    name: String,
}

impl CurrentTimeNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for CurrentTimeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("输出当前本地时间的字符串表示")
    }

    node_input![];

    node_output![
        port! { name = "time", ty = String, desc = "当前本地时间字符串，格式为 YYYY-MM-DD HH:MM:SS" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        outputs.insert(
            "time".to_string(),
            DataValue::String(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::CurrentTimeNode;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn current_time_node_outputs_non_empty_string() {
        let mut node = CurrentTimeNode::new("now", "当前时间");
        let outputs = node
            .execute(HashMap::new())
            .expect("current time node should execute");

        match outputs.get("time") {
            Some(DataValue::String(value)) => assert!(!value.is_empty()),
            other => panic!("unexpected time output: {:?}", other),
        }
    }
}
