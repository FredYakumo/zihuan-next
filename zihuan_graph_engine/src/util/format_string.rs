use crate::{node_output, DataType, DataValue, Node, Port};
use std::collections::{HashMap, HashSet};
use zihuan_core::error::Result;

fn extract_variables(template: &str) -> Vec<String> {
    let mut vars = vec![];
    let mut seen = HashSet::new();
    let mut pos = 0;
    while let Some(rel) = template[pos..].find("${") {
        let start = pos + rel + 2;
        if let Some(end_rel) = template[start..].find('}') {
            let name = template[start..start + end_rel].trim().to_string();
            if !name.is_empty() && seen.insert(name.clone()) {
                vars.push(name);
            }
            pos = start + end_rel + 1;
        } else {
            break;
        }
    }
    vars
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncompleteVariable {
    pub open_index: usize,
    pub cursor_index: usize,
    pub prefix: String,
}

fn clamp_to_char_boundary(text: &str, byte_offset: usize) -> usize {
    let mut offset = byte_offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn find_incomplete_variable_at(
    text: &str,
    cursor_byte_offset: usize,
) -> Option<IncompleteVariable> {
    let cursor_index = clamp_to_char_boundary(text, cursor_byte_offset);
    let before_cursor = &text[..cursor_index];
    let open_index = before_cursor.rfind("${")?;
    let prefix = &before_cursor[open_index + 2..];
    if prefix.contains('}') {
        return None;
    }

    Some(IncompleteVariable {
        open_index,
        cursor_index,
        prefix: prefix.to_string(),
    })
}

pub fn complete_incomplete_variable_at(
    text: &str,
    cursor_byte_offset: usize,
    suggestion: &str,
) -> Option<String> {
    let ctx = find_incomplete_variable_at(text, cursor_byte_offset)?;

    // If there is already a closing brace after the cursor, replace the full
    // `${...}` segment. Otherwise replace only the incomplete prefix.
    let replacement_end = if let Some(close_rel) = text[ctx.cursor_index..].find('}') {
        ctx.cursor_index + close_rel + 1
    } else {
        ctx.cursor_index
    };

    Some(format!(
        "{}${{{}}}{}",
        &text[..ctx.open_index],
        suggestion,
        &text[replacement_end..]
    ))
}

pub struct FormatStringNode {
    id: String,
    name: String,
    template: String,
    variables: Vec<String>,
}

impl FormatStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            template: String::new(),
            variables: vec![],
        }
    }
}

impl Node for FormatStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("通过 ${变量名} 模板语法将输入变量格式化为字符串")
    }

    fn has_dynamic_input_ports(&self) -> bool {
        true
    }

    fn input_ports(&self) -> Vec<Port> {
        // "template" must always be present so the registry can pass it to
        // apply_inline_config before the dynamic variable ports are known.
        let mut ports = vec![Port::new("template", DataType::String)
            .with_description("格式化模板字符串，使用 ${变量名} 语法引用输入变量")
            .optional()
            .hidden()];
        ports.extend(self.variables.iter().map(|var| {
            Port::new(var.clone(), DataType::Any).with_description(format!("变量 {var}"))
        }));
        ports
    }

    node_output![port! { name = "output", ty = String, desc = "格式化后的字符串" },];

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::String(template)) = inline_values.get("template") {
            self.template = template.clone();
            self.variables = extract_variables(template);
        }
        Ok(())
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut result = self.template.clone();
        for var in &self.variables {
            let value = match inputs.get(var) {
                Some(v) => v.to_display_string(),
                None => String::new(),
            };
            result = result.replace(&format!("${{{var}}}"), &value);
        }

        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), DataValue::String(result));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
