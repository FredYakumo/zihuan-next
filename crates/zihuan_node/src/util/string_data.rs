use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;

// Global context kept for backwards compatibility (no longer used by StringDataNode itself)
pub static STRING_DATA_CONTEXT: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub struct StringDataNode {
    id: String,
    name: String,
    value: String,
}

impl StringDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            value: String::new(),
        }
    }
}

impl Node for StringDataNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("String data source with UI input field")
    }

    node_input![];

    node_output![port! { name = "text", ty = String, desc = "Output string from UI input" },];

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::String(s)) = inline_values.get("text") {
            self.value = s.clone();
        }
        Ok(())
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        outputs.insert("text".to_string(), DataValue::String(self.value.clone()));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
