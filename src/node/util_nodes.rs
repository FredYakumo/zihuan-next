use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// 数据转换节点
pub struct DataTransformNode {
    id: String,
    name: String,
}

impl DataTransformNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for DataTransformNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Transform data between different formats")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("input", DataType::Json)
                .with_description("Input data in any format"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("output", DataType::Json)
                .with_description("Transformed data"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(input_data) = inputs.get("input") {
            outputs.insert(
                "output".to_string(),
                input_data.clone(),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// 条件分支节点
pub struct ConditionalNode {
    id: String,
    name: String,
}

impl ConditionalNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConditionalNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Conditional branching based on input condition")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("condition", DataType::Boolean)
                .with_description("Condition to evaluate"),
            Port::new("true_value", DataType::Json)
                .with_description("Value to output if condition is true"),
            Port::new("false_value", DataType::Json)
                .with_description("Value to output if condition is false"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("result", DataType::Json)
                .with_description("Selected value based on condition"),
            Port::new("branch_taken", DataType::String)
                .with_description("Which branch was taken"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::Boolean(condition)) = inputs.get("condition") {
            let (result, branch) = if *condition {
                (
                    inputs.get("true_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "true",
                )
            } else {
                (
                    inputs.get("false_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "false",
                )
            };

            outputs.insert("result".to_string(), result);
            outputs.insert("branch_taken".to_string(), DataValue::String(branch.to_string()));
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// JSON 解析节点
pub struct JsonParserNode {
    id: String,
    name: String,
}

impl JsonParserNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for JsonParserNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Parse JSON string to structured data")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("json_string", DataType::String)
                .with_description("JSON string to parse"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("parsed", DataType::Json)
                .with_description("Parsed JSON object"),
            Port::new("success", DataType::Boolean)
                .with_description("Whether parsing was successful"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::String(json_str)) = inputs.get("json_string") {
            match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(parsed) => {
                    outputs.insert("parsed".to_string(), DataValue::Json(parsed));
                    outputs.insert("success".to_string(), DataValue::Boolean(true));
                }
                Err(_) => {
                    outputs.insert("parsed".to_string(), DataValue::Json(serde_json::json!(null)));
                    outputs.insert("success".to_string(), DataValue::Boolean(false));
                }
            }
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// 数据聚合节点
pub struct AggregatorNode {
    id: String,
    name: String,
}

impl AggregatorNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AggregatorNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Aggregate multiple inputs into a single output")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("input1", DataType::Json)
                .with_description("First input"),
            Port::new("input2", DataType::Json)
                .with_description("Second input"),
            Port::new("input3", DataType::Json)
                .with_description("Third input (optional)"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("aggregated", DataType::Json)
                .with_description("Aggregated result"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        let mut result = serde_json::Map::new();

        for (key, value) in inputs.iter() {
            result.insert(key.clone(), value.to_json());
        }

        outputs.insert(
            "aggregated".to_string(),
            DataValue::Json(serde_json::Value::Object(result)),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// 延迟节点
pub struct DelayNode {
    id: String,
    name: String,
}

impl DelayNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for DelayNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Delay data flow by specified duration")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("data", DataType::Json)
                .with_description("Data to pass through"),
            Port::new("delay_ms", DataType::Integer)
                .with_description("Delay in milliseconds"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("output", DataType::Json)
                .with_description("Data after delay"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(data) = inputs.get("data") {
            // 注意：实际的延迟需要异步实现
            // 这里只是简单地传递数据
            outputs.insert("output".to_string(), data.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
