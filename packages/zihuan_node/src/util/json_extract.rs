use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{node_input, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};

const FIELDS_CONFIG_PORT: &str = "fields_config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonExtractFieldDef {
    pub name: String,
    pub data_type: DataType,
}

fn validate_field_definitions(field_definitions: &[JsonExtractFieldDef]) -> Result<()> {
    let mut field_names = HashSet::new();

    for field in field_definitions {
        let field_name = field.name.trim();
        if field_name.is_empty() {
            return Err(Error::ValidationError("提取字段名不能为空".to_string()));
        }
        if !field_names.insert(field_name.to_string()) {
            return Err(Error::ValidationError(format!(
                "提取字段名重复：{field_name}"
            )));
        }
    }

    Ok(())
}

fn json_value_to_data_value(json: &Value, target_type: &DataType) -> Option<DataValue> {
    match (json, target_type) {
        (_, DataType::Any) => match json {
            Value::String(value) => Some(DataValue::String(value.clone())),
            Value::Number(value) => value
                .as_i64()
                .map(DataValue::Integer)
                .or_else(|| value.as_f64().map(DataValue::Float)),
            Value::Bool(value) => Some(DataValue::Boolean(*value)),
            _ => Some(DataValue::Json(json.clone())),
        },
        (Value::String(value), DataType::String) => Some(DataValue::String(value.clone())),
        (Value::String(value), DataType::Password) => Some(DataValue::Password(value.clone())),
        (Value::String(value), DataType::Boolean) => match value.as_str() {
            "true" => Some(DataValue::Boolean(true)),
            "false" => Some(DataValue::Boolean(false)),
            _ => None,
        },
        (Value::String(value), DataType::Integer) => value.parse().ok().map(DataValue::Integer),
        (Value::String(value), DataType::Float) => value.parse().ok().map(DataValue::Float),
        (Value::Number(value), DataType::Integer) => value.as_i64().map(DataValue::Integer),
        (Value::Number(value), DataType::Float) => value.as_f64().map(DataValue::Float),
        (Value::Bool(value), DataType::Boolean) => Some(DataValue::Boolean(*value)),
        (_, DataType::Json) => Some(DataValue::Json(json.clone())),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct JsonExtractNode {
    id: String,
    name: String,
    field_definitions: Vec<JsonExtractFieldDef>,
}

impl JsonExtractNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            field_definitions: Vec::new(),
        }
    }

    fn set_field_definitions(&mut self, field_definitions: Vec<JsonExtractFieldDef>) -> Result<()> {
        validate_field_definitions(&field_definitions)?;
        self.field_definitions = field_definitions;
        Ok(())
    }

    fn output_ports_from_fields(field_definitions: &[JsonExtractFieldDef]) -> Vec<Port> {
        field_definitions
            .iter()
            .map(|field| {
                Port::new(field.name.clone(), field.data_type.clone())
                    .with_description(format!("从输入 JSON 中提取字段 '{}'", field.name))
            })
            .collect()
    }
}

impl Node for JsonExtractNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从输入 JSON 中按字段名提取值，并根据字段配置动态生成输出端口")
    }

    fn has_dynamic_output_ports(&self) -> bool {
        true
    }

    node_input![
        port! { name = "json", ty = Json, desc = "待提取字段的 JSON 对象" },
        port! { name = "fields_config", ty = Json, desc = "提取字段配置，由字段编辑器维护", optional },
    ];

    fn output_ports(&self) -> Vec<Port> {
        Self::output_ports_from_fields(&self.field_definitions)
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(FIELDS_CONFIG_PORT) {
            Some(DataValue::Json(value)) => {
                if value.is_null() {
                    self.field_definitions.clear();
                    return Ok(());
                }

                let parsed = serde_json::from_value::<Vec<JsonExtractFieldDef>>(value.clone())
                    .map_err(|e| Error::ValidationError(format!("Invalid fields_config: {e}")))?;
                self.set_field_definitions(parsed)
            }
            Some(other) => Err(Error::ValidationError(format!(
                "fields_config expects Json, got {}",
                other.data_type()
            ))),
            None => {
                self.field_definitions.clear();
                Ok(())
            }
        }
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        if let Some(DataValue::Json(value)) = inputs.get(FIELDS_CONFIG_PORT) {
            let parsed = serde_json::from_value::<Vec<JsonExtractFieldDef>>(value.clone())
                .map_err(|e| Error::ValidationError(format!("Invalid fields_config: {e}")))?;
            self.set_field_definitions(parsed)?;
        }

        let json = match inputs.get("json") {
            Some(DataValue::Json(value)) => value,
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: json".to_string(),
                ));
            }
        };

        let object = json.as_object().ok_or_else(|| {
            Error::ValidationError("json_extract 节点要求输入 JSON 必须为对象".to_string())
        })?;

        let mut outputs = HashMap::new();
        for field in &self.field_definitions {
            let raw_value = object.get(&field.name).ok_or_else(|| {
                Error::ValidationError(format!("JSON 中不存在字段 '{}'", field.name))
            })?;

            let typed_value =
                json_value_to_data_value(raw_value, &field.data_type).ok_or_else(|| {
                    Error::ValidationError(format!(
                        "字段 '{}' 无法转换为类型 {}",
                        field.name, field.data_type
                    ))
                })?;

            outputs.insert(field.name.clone(), typed_value);
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

