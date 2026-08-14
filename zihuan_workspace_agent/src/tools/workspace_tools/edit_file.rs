use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};

use super::shared::{
	content_hash, json_error, path_resource, resolve_tool_path, success_json, text_lines,
};

pub(crate) const DEFAULT_TOOL_EDIT_FILE: &str = "edit_file";

#[derive(Debug, Deserialize)]
struct EditFileArgs {
	path: String,
	content_hash: String,
	edits: Vec<LineEditSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct LineEditSpec {
	start_line: usize,
	end_line: usize,
	expected_lines: Vec<String>,
	replacement_lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct EditFileBrainTool {
	pub(crate) workspace_path: Option<PathBuf>,
}

impl BrainTool for EditFileBrainTool {
	fn spec(&self) -> Arc<dyn FunctionTool> {
		Arc::new(StaticFunctionToolSpec {
			name: DEFAULT_TOOL_EDIT_FILE,
			description: "Replace or delete existing UTF-8 file lines using 1-based inclusive ranges. First call read_file, then pass its full-file content_hash and the exact expected_lines for every range. The entire request is rejected without writing if the file changed, expected content differs, or ranges overlap.",
			parameters: json!({
				"type": "object",
				"properties": {
					"path": { "type": "string" },
					"content_hash": {
						"type": "string",
						"description": "Full-file content_hash returned by the most recent read_file call"
					},
					"edits": {
						"type": "array",
						"minItems": 1,
						"items": {
							"type": "object",
							"properties": {
								"start_line": { "type": "integer", "minimum": 1 },
								"end_line": { "type": "integer", "minimum": 1 },
								"expected_lines": {
									"type": "array",
									"items": { "type": "string" },
									"description": "Exact current lines in the inclusive range"
								},
								"replacement_lines": {
									"type": "array",
									"items": { "type": "string" }
								}
							},
							"required": [
								"start_line",
								"end_line",
								"expected_lines",
								"replacement_lines"
							]
						}
					}
				},
				"required": ["path", "content_hash", "edits"]
			}),
		})
	}

	fn execute(&self, _: &str, arguments: &Value) -> String {
		let args: EditFileArgs = match serde_json::from_value(arguments.clone()) {
			Ok(value) => value,
			Err(err) => return json_error(format!("invalid edit_file arguments: {err}")),
		};
		let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
			Ok(value) => value,
			Err(err) => return json_error(err.to_string()),
		};
		let original = match fs::read_to_string(&path) {
			Ok(value) => value,
			Err(err) => return json_error(format!("failed to read file '{}': {err}", path.display())),
		};
		let before_hash = content_hash(&original);
		let original_lines = text_lines(&original);

		// Symptom: an old range remained in bounds but landed in another function after prior edits shifted lines.
		// Cause: the former tool validated only numeric bounds. Matching the read_file hash proves that the
		// coordinates were calculated from the same file version; a mismatch aborts before any mutation.
		if args.content_hash != before_hash {
			return success_json(json!({
				"ok": false,
				"error_code": "stale_file",
				"error": "file content changed since read_file; read the file again before editing",
				"path": path.display().to_string(),
				"expected_content_hash": args.content_hash,
				"actual_content_hash": before_hash,
				"line_count": original_lines.len()
			}));
		}
		if args.edits.is_empty() {
			return json_error("edits must contain at least one edit");
		}

		let mut edits = args.edits;
		edits.sort_by(|left, right| {
			left.start_line
				.cmp(&right.start_line)
				.then_with(|| left.end_line.cmp(&right.end_line))
		});

		for (index, edit) in edits.iter().enumerate() {
			if edit.start_line == 0 || edit.end_line == 0 || edit.start_line > edit.end_line {
				return json_error(format!(
					"invalid line range: start_line={} end_line={}",
					edit.start_line, edit.end_line
				));
			}
			if edit.end_line > original_lines.len() {
				return json_error(format!(
					"line range [{}-{}] is out of bounds for file '{}' with {} lines",
					edit.start_line,
					edit.end_line,
					path.display(),
					original_lines.len()
				));
			}
			if index > 0 && edit.start_line <= edits[index - 1].end_line {
				return success_json(json!({
					"ok": false,
					"error_code": "overlapping_edits",
					"error": "edit ranges must not overlap",
					"path": path.display().to_string(),
					"first_range": [edits[index - 1].start_line, edits[index - 1].end_line],
					"second_range": [edit.start_line, edit.end_line]
				}));
			}

			// Symptom: a request intended to change one return-type line deleted the surrounding function
			// signature because its numeric range was too broad. A matching file hash cannot catch that model
			// mistake, but requiring the exact old lines proves the requested boundaries contain what it expects.
			let actual_lines = &original_lines[edit.start_line - 1..edit.end_line];
			let expected_matches = actual_lines
				.iter()
				.map(|line| *line)
				.eq(edit.expected_lines.iter().map(String::as_str));
			if !expected_matches {
				return success_json(json!({
					"ok": false,
					"error_code": "expected_lines_mismatch",
					"error": "target lines do not match expected_lines; read the file again and use a smaller exact range",
					"path": path.display().to_string(),
					"start_line": edit.start_line,
					"end_line": edit.end_line,
					"expected_lines": edit.expected_lines,
					"actual_lines": actual_lines
				}));
			}
		}

		// All edits are validated against the immutable original before the first splice. This prevents one
		// failed range from leaving earlier ranges written, and descending application keeps lower edits from
		// shifting the original coordinates of higher edits within the same request.
		let old_line_count = original_lines.len();
		let trailing_newline = original.ends_with('\n');
		let line_ending = if original.contains("\r\n") { "\r\n" } else { "\n" };
		let mut rewritten_lines = original_lines
			.iter()
			.map(|line| (*line).to_string())
			.collect::<Vec<_>>();
		let applied_ranges = edits
			.iter()
			.map(|edit| {
				json!({
					"start_line": edit.start_line,
					"end_line": edit.end_line,
					"removed_lines": edit.expected_lines.len(),
					"added_lines": edit.replacement_lines.len()
				})
			})
			.collect::<Vec<_>>();

		for edit in edits.into_iter().rev() {
			rewritten_lines.splice(edit.start_line - 1..edit.end_line, edit.replacement_lines);
		}

		let mut rewritten = rewritten_lines.join(line_ending);
		if trailing_newline && !rewritten.is_empty() {
			rewritten.push_str(line_ending);
		}
		if let Err(err) = fs::write(&path, &rewritten) {
			return json_error(format!("failed to write edited file '{}': {err}", path.display()));
		}

		success_json(json!({
			"ok": true,
			"path": path.display().to_string(),
			"before_content_hash": before_hash,
			"after_content_hash": content_hash(&rewritten),
			"old_line_count": old_line_count,
			"line_count": rewritten_lines.len(),
			"applied_ranges": applied_ranges
		}))
	}

	fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
		serde_json::from_value::<EditFileArgs>(arguments.clone())
			.map(|args| path_resource(self.workspace_path.as_deref(), &args.path, true))
			.unwrap_or(ToolExecutionResource::Exclusive)
	}
}
