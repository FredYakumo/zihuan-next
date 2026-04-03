use super::{inline_port_key, InlinePortValue, NodeRenderer};
use zihuan_node::graph_io::NodeGraphDefinition;
use std::collections::HashMap;

pub struct FormatStringRenderer;

impl NodeRenderer for FormatStringRenderer {
    fn get_preview_text(
        node_id: &str,
        graph: &NodeGraphDefinition,
        inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        // Try inline_inputs first (live edits), then fall back to saved inline_values
        let key = inline_port_key(node_id, "template");
        let text = match inline_inputs.get(&key) {
            Some(InlinePortValue::Text(t)) => t.clone(),
            _ => graph
                .nodes
                .iter()
                .find(|n| n.id == node_id)
                .and_then(|n| n.inline_values.get("template"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        };

        if text.is_empty() {
            "(空模板)".to_string()
        } else if text.chars().count() > 40 {
            format!(
                "{}…",
                &text[..text
                    .char_indices()
                    .nth(40)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len())]
            )
        } else {
            text
        }
    }

    fn handles_node_type(node_type: &str) -> bool {
        node_type == "format_string"
    }
}
