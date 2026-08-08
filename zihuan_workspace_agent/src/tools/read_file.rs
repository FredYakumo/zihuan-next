use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use base64::Engine;
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub const DEFAULT_TOOL_READ_FILE: &str = "read_file";
#[derive(Debug, Clone, Deserialize)] struct ReadFileArgs {
    path: String,
    #[serde(default)] start_line: Option<usize>,
    #[serde(default)] end_line: Option<usize>,
    #[serde(default)] encoding: Option<String>,
    #[serde(default)] byte_start: Option<usize>,
    #[serde(default)] byte_end: Option<usize>,
}
#[derive(Debug, Clone)] pub struct ReadFileBrainTool { pub(crate) workspace_path: Option<PathBuf> }
impl BrainTool for ReadFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_READ_FILE, description: "Read a UTF-8 text file by line range or a binary-safe base64 byte range", parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1},"encoding":{"type":"string","enum":["utf8","base64"]},"byte_start":{"type":"integer","minimum":0},"byte_end":{"type":"integer","minimum":0}},"required":["path"]}) }) }
    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: ReadFileArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid read_file arguments: {err}")) };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        let encoding = args.encoding.as_deref().unwrap_or("utf8");
        if encoding == "base64" {
            let bytes = match fs::read(&path) { Ok(bytes) => bytes, Err(err) => return json_error(format!("failed to read file '{}': {err}", path.display())) };
            let start = args.byte_start.unwrap_or(0);
            let end = args.byte_end.unwrap_or(bytes.len());
            if start > end || end > bytes.len() { return json_error(format!("invalid byte range [{start}-{end}] for file with {} bytes", bytes.len())); }
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes[start..end]);
            return success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"encoding":"base64","byte_start":start,"byte_end":end,"total_bytes":bytes.len(),"content":encoded}));
        }
        if encoding != "utf8" { return json_error("encoding must be utf8 or base64"); }
        let content = match fs::read_to_string(&path) { Ok(content) => content, Err(err) => return json_error(format!("failed to read file '{}': {err}", path.display())) };
        let lines: Vec<&str> = content.lines().collect(); let total_lines = lines.len();
        let start_line = args.start_line.unwrap_or(1); let end_line = args.end_line.unwrap_or_else(|| total_lines.max(1));
        if start_line == 0 || end_line == 0 || start_line > end_line { return json_error(format!("invalid line range: start_line={start_line} end_line={end_line}")); }
        if total_lines == 0 { if args.start_line.is_some() || args.end_line.is_some() { return json_error("line range is out of bounds for an empty file"); } return success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"start_line":0,"end_line":0,"total_lines":0,"content":""})); }
        if start_line > total_lines || end_line > total_lines { return json_error(format!("line range [{start_line}-{end_line}] is out of bounds for file '{}' with {total_lines} lines", path.display())); }
        success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"start_line":start_line,"end_line":end_line,"total_lines":total_lines,"content":lines[start_line-1..end_line].join("\n")}))
    }
    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource { serde_json::from_value::<ReadFileArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive) }
}
