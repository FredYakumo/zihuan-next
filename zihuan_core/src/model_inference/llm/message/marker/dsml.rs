//! DeepSeek DSML tool-call marker parsing.
//!
//! Problem example: Some OpenAI-compatible DeepSeek models emit tool calls as assistant text:
//!
//! ```text
//! <|DSML|tool_calls><|DSML|invoke name="read_file"><|DSML|parameter name="path" string="true">src/lib.rs</|DSML|parameter></|DSML|invoke></|DSML|tool_calls>
//! ```
//!
//! Without this parser, the protocol is shown to users and the tool is never invoked.
//! This module removes the marker from visible content and converts it to an internal
//! `ToolCalls { function: "read_file", arguments: { "path": "src/lib.rs" } }` value.

use serde_json::{Map, Value};

use crate::model_inference::llm::tooling::{ToolCalls, ToolCallsFuncSpec};

pub(crate) struct DsmlContentParser {
    pending: String,
    pub(crate) tool_calls: Vec<ToolCalls>,
}

/// Purpose:
///     Merge native OpenAI calls with DSML calls and assign collision-free IDs.
pub(crate) fn merge_tool_calls(
    mut native_tool_calls: Vec<ToolCalls>,
    mut dsml_tool_calls: Vec<ToolCalls>,
) -> Vec<ToolCalls> {
    let mut call_ids = native_tool_calls
        .iter()
        .map(|tool_call| tool_call.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for (index, tool_call) in dsml_tool_calls.iter_mut().enumerate() {
        let base_id = format!("dsml_tool_call_{index}");
        let mut candidate = base_id.clone();
        let mut suffix = 1;
        while call_ids.contains(&candidate) {
            candidate = format!("{base_id}_{suffix}");
            suffix += 1;
        }
        tool_call.id = candidate.clone();
        call_ids.insert(candidate);
    }
    native_tool_calls.append(&mut dsml_tool_calls);
    native_tool_calls
}

enum DsmlToolCallsBlock {
    Complete { consumed: usize, tool_calls: Vec<ToolCalls> },
    Incomplete,
    Invalid,
}

struct DsmlTag {
    closing: bool,
    name: String,
    attributes: Map<String, Value>,
}

impl DsmlContentParser {
    /// Purpose:
    ///     Create an empty incremental DSML content parser.
    pub(crate) fn new() -> Self {
        Self {
            pending: String::new(),
            tool_calls: Vec::new(),
        }
    }

    /// Purpose:
    ///     Consume one content delta and return text safe to expose immediately.
    pub(crate) fn push(&mut self, piece: &str) -> Vec<String> {
        self.pending.push_str(piece);
        self.process(false)
    }

    /// Purpose:
    ///     Flush pending content after the stream ends, preserving incomplete DSML.
    pub(crate) fn finish(&mut self) -> Vec<String> {
        self.process(true)
    }

    /// Purpose:
    ///     Separate ordinary content from complete DSML tool-call blocks.
    fn process(&mut self, finish: bool) -> Vec<String> {
        let mut emitted = Vec::new();

        loop {
            let Some(start) = find_dsml_tool_calls_start(&self.pending) else {
                let retained = if finish {
                    0
                } else {
                    dsml_start_prefix_len(&self.pending)
                };
                let emitted_len = self.pending.len() - retained;
                if emitted_len > 0 {
                    emitted.push(self.pending[..emitted_len].to_string());
                    self.pending = self.pending[emitted_len..].to_string();
                }
                break;
            };

            if start > 0 {
                emitted.push(self.pending[..start].to_string());
                self.pending = self.pending[start..].to_string();
                continue;
            }

            match parse_dsml_tool_calls_block(&self.pending) {
                DsmlToolCallsBlock::Complete {
                    consumed,
                    tool_calls,
                } => {
                    self.tool_calls.extend(tool_calls);
                    self.pending = self.pending[consumed..].to_string();
                }
                DsmlToolCallsBlock::Incomplete if !finish => break,
                DsmlToolCallsBlock::Incomplete | DsmlToolCallsBlock::Invalid => {
                    let first_len = self.pending.chars().next().map(char::len_utf8).unwrap_or(0);
                    emitted.push(self.pending[..first_len].to_string());
                    self.pending = self.pending[first_len..].to_string();
                }
            }
        }

        emitted
    }
}

/// Purpose:
///     Locate the next opening DSML `tool_calls` marker in content.
fn find_dsml_tool_calls_start(text: &str) -> Option<usize> {
    text.char_indices().find_map(|(index, ch)| {
        if ch != '<' {
            return None;
        }

        let (_, tag) = parse_dsml_tag(text, index)?;
        (!tag.closing && tag.name == "tool_calls").then_some(index)
    })
}

/// Purpose:
///     Retain a trailing partial DSML opening marker for the next stream delta.
fn dsml_start_prefix_len(text: &str) -> usize {
    let Some(start) = text.rfind('<') else {
        return 0;
    };
    let suffix = &text[start..];
    let normalized = suffix.chars().filter(|ch| !ch.is_whitespace()).collect::<String>();
    let prefixes = ["<|DSML|tool_calls", "<｜DSML｜tool_calls"];
    if prefixes.iter().any(|prefix| prefix.starts_with(&normalized)) {
        suffix.len()
    } else {
        0
    }
}

/// Purpose:
///     Parse a complete DSML tool-call wrapper or classify it as incomplete or invalid.
fn parse_dsml_tool_calls_block(text: &str) -> DsmlToolCallsBlock {
    let Some((mut position, opening_tag)) = parse_dsml_tag(text, 0) else {
        return DsmlToolCallsBlock::Incomplete;
    };
    if opening_tag.closing || opening_tag.name != "tool_calls" {
        return DsmlToolCallsBlock::Invalid;
    }

    let mut tool_calls = Vec::new();
    loop {
        position += text[position..].len() - text[position..].trim_start().len();
        if position >= text.len() {
            return DsmlToolCallsBlock::Incomplete;
        }

        let Some((tag_end, tag)) = parse_dsml_tag(text, position) else {
            return DsmlToolCallsBlock::Incomplete;
        };
        if tag.closing && tag.name == "tool_calls" {
            return if tool_calls.is_empty() {
                DsmlToolCallsBlock::Invalid
            } else {
                DsmlToolCallsBlock::Complete {
                    consumed: tag_end,
                    tool_calls,
                }
            };
        }
        if tag.closing || tag.name != "invoke" {
            return DsmlToolCallsBlock::Invalid;
        }

        let Some(name) = tag.attributes.get("name").and_then(Value::as_str) else {
            return DsmlToolCallsBlock::Invalid;
        };
        let mut arguments = Map::new();
        position = tag_end;

        loop {
            position += text[position..].len() - text[position..].trim_start().len();
            let Some((parameter_end, parameter_tag)) = parse_dsml_tag(text, position) else {
                return DsmlToolCallsBlock::Incomplete;
            };
            if parameter_tag.closing && parameter_tag.name == "invoke" {
                position = parameter_end;
                break;
            }
            if parameter_tag.closing || parameter_tag.name != "parameter" {
                return DsmlToolCallsBlock::Invalid;
            }

            let Some(parameter_name) = parameter_tag.attributes.get("name").and_then(Value::as_str) else {
                return DsmlToolCallsBlock::Invalid;
            };
            let content_start = parameter_end;
            let Some(closing_start) = find_next_dsml_tag(text, content_start, "parameter", true) else {
                return DsmlToolCallsBlock::Incomplete;
            };
            let Some((closing_end, closing_tag)) = parse_dsml_tag(text, closing_start) else {
                return DsmlToolCallsBlock::Incomplete;
            };
            if !closing_tag.closing || closing_tag.name != "parameter" {
                return DsmlToolCallsBlock::Invalid;
            }

            let raw_value = &text[content_start..closing_start];
            let value_is_string = parameter_tag
                .attributes
                .get("string")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let value = if value_is_string {
                Value::String(raw_value.to_string())
            } else {
                serde_json::from_str(raw_value.trim())
                    .unwrap_or_else(|_| Value::String(raw_value.to_string()))
            };
            arguments.insert(parameter_name.to_string(), value);
            position = closing_end;
        }

        tool_calls.push(ToolCalls {
            id: format!("dsml_tool_call_{}", tool_calls.len()),
            type_name: "function".to_string(),
            function: ToolCallsFuncSpec {
                name: name.to_string(),
                arguments: Value::Object(arguments),
            },
        });
    }
}

/// Purpose:
///     Find the next DSML tag with the requested name and closing state.
fn find_next_dsml_tag(
    text: &str,
    mut position: usize,
    expected_name: &str,
    expected_closing: bool,
) -> Option<usize> {
    while position < text.len() {
        let relative = text[position..].find('<')?;
        let tag_start = position + relative;
        if let Some((_, tag)) = parse_dsml_tag(text, tag_start) {
            if tag.name == expected_name && tag.closing == expected_closing {
                return Some(tag_start);
            }
        }
        position = tag_start + '<'.len_utf8();
    }
    None
}

/// Purpose:
///     Parse one ASCII or full-width-delimiter DSML tag and its attributes.
fn parse_dsml_tag(text: &str, start: usize) -> Option<(usize, DsmlTag)> {
    if !text[start..].starts_with('<') {
        return None;
    }
    let close_offset = text[start..].find('>')?;
    let end = start + close_offset + 1;
    let mut body = text[start + 1..start + close_offset].trim_start();
    let closing = body.strip_prefix('/').is_some();
    if closing {
        body = body[1..].trim_start();
    }
    body = body.strip_prefix('|').or_else(|| body.strip_prefix('｜'))?.trim_start();
    let rest = body.strip_prefix("DSML")?;
    let rest = rest.trim_start().strip_prefix('|').or_else(|| rest.trim_start().strip_prefix('｜'))?.trim_start();
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = &rest[..name_end];
    if name.is_empty() {
        return None;
    }

    let attributes = parse_dsml_attributes(rest[name_end..].trim())?;
    Some((
        end,
        DsmlTag {
            closing,
            name: name.to_string(),
            attributes,
        },
    ))
}

/// Purpose:
///     Parse quoted DSML attributes into JSON-compatible values.
fn parse_dsml_attributes(text: &str) -> Option<Map<String, Value>> {
    let mut attributes = Map::new();
    let mut position = 0;
    while position < text.len() {
        position += text[position..].len() - text[position..].trim_start().len();
        if position == text.len() {
            break;
        }
        let remaining = &text[position..];
        let name_end = remaining.find(|ch: char| ch == '=' || ch.is_whitespace())?;
        let name = &remaining[..name_end];
        position += name_end;
        position += text[position..].len() - text[position..].trim_start().len();
        if !text[position..].starts_with('=') {
            return None;
        }
        position += 1;
        position += text[position..].len() - text[position..].trim_start().len();
        let quote = text[position..].chars().next()?;
        if quote != '\'' && quote != '"' {
            return None;
        }
        position += quote.len_utf8();
        let value_end = text[position..].find(quote)?;
        let value = &text[position..position + value_end];
        position += value_end + quote.len_utf8();
        let value = match value {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(value.to_string()),
        };
        attributes.insert(name.to_string(), value);
    }
    Some(attributes)
}
