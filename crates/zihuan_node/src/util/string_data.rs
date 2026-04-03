use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;

// Global context for string_data nodes to access UI input values
pub static STRING_DATA_CONTEXT: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub struct StringDataNode {
    id: String,
    name: String,
}

impl StringDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
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

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        // StringDataNode gets its value from the global context (set by UI layer before execution)
        let mut outputs = HashMap::new();
        let value = {
            let context = STRING_DATA_CONTEXT.read().unwrap();
            context.get(&self.id).cloned().unwrap_or_default()
        };
        outputs.insert("text".to_string(), DataValue::String(value));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
