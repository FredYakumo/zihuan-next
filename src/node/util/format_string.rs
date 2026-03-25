use crate::error::Result;
use crate::node::{node_output, DataType, DataValue, Node, Port};
use std::collections::{HashMap, HashSet};

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

pub fn find_incomplete_variable(text: &str) -> Option<String> {
    let last_open = text.rfind("${")?;
    let after = &text[last_open + 2..];
    if after.contains('}') {
        None
    } else {
        Some(after.to_string())
    }
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

    fn input_ports(&self) -> Vec<Port> {
        self.variables
            .iter()
            .map(|var| {
                Port::new(var.clone(), DataType::String)
                    .with_description(format!("变量 {var}"))
            })
            .collect()
    }

    node_output![
        port! { name = "output", ty = String, desc = "格式化后的字符串" },
    ];

    fn apply_inline_config(
        &mut self,
        inline_values: &HashMap<String, DataValue>,
    ) -> Result<()> {
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
                Some(DataValue::String(s)) => s.clone(),
                Some(v) => format!("{v:?}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_variables_basic() {
        let vars = extract_variables("Hello ${name}, score: ${score}");
        assert_eq!(vars, vec!["name", "score"]);
    }

    #[test]
    fn extract_variables_deduplication() {
        let vars = extract_variables("${a} ${b} ${a}");
        assert_eq!(vars, vec!["a", "b"]);
    }

    #[test]
    fn find_incomplete_variable_detects_open() {
        assert_eq!(
            find_incomplete_variable("Hello ${na"),
            Some("na".to_string())
        );
    }

    #[test]
    fn find_incomplete_variable_none_when_closed() {
        assert_eq!(find_incomplete_variable("Hello ${name}"), None);
    }

    #[test]
    fn execute_substitutes_variables() {
        let mut node = FormatStringNode::new("id", "name");
        node.template = "Hi ${name}!".to_string();
        node.variables = vec!["name".to_string()];

        let mut inputs = HashMap::new();
        inputs.insert("name".to_string(), DataValue::String("World".to_string()));

        let outputs = node.execute(inputs).unwrap();
        match outputs.get("output") {
            Some(DataValue::String(s)) => assert_eq!(s, "Hi World!"),
            other => panic!("unexpected output: {:?}", other),
        }
    }
}
