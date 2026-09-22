use super::shared::json_error;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use zihuan_core::agent::tools::{Tool, ToolExecutionOutput, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
use zihuan_core::workspace::AskUserRequest;
pub(crate) const DEFAULT_TOOL_ASK_USER: &str = "ask_user";
#[derive(Debug, Clone, Default)]
pub(crate) struct AskUserTool;
#[derive(Debug, Deserialize)]
struct AskUserArgs {
    question: String,
    #[serde(default)]
    details: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
}
impl Tool for AskUserTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_ASK_USER,
            description: "Ask the dashboard user for missing details and pause until they reply. \
                Always provide 2-3 candidate answers in `options`, each written as the reply the \
                user would send: when the question has explicit choices, list them; when it is \
                open-ended, list the 2-3 most likely interpretations or answers, because the \
                dashboard renders them as one-click choices next to a free-form input and a \
                \"暂时不想回答\" choice. If the reply is \"用户暂未回答\", the user declined to \
                answer: continue with your best assumption and make that assumption explicit \
                instead of asking again.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The single question that blocks progress."
                    },
                    "details": {
                        "type": "string",
                        "description": "Optional background or context shown under the question."
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "2-3 short candidate answers offered as one-click choices, each written as the reply the user would send."
                    },
                    "placeholder": {
                        "type": "string",
                        "description": "Optional hint for the free-form answer input; only shown when `options` is omitted."
                    }
                },
                "required": ["question"]
            }),
        })
    }
    fn execute_with_outcome(&self, _: &str, a: &Value) -> ToolExecutionOutput {
        let args: AskUserArgs = match serde_json::from_value(a.clone()) {
            Ok(v) => v,
            Err(e) => {
                return ToolExecutionOutput::text(json_error(format!(
                    "invalid ask_user arguments: {e}"
                )))
            }
        };
        let question = args.question.trim().to_string();
        if question.is_empty() {
            return ToolExecutionOutput::text(json_error("question must not be empty"));
        }
        let options = normalize_options(args.options);
        let request = AskUserRequest {
            question: question.clone(),
            details: args.details.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            placeholder: args.placeholder.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            options,
            command_confirmation: None,
            tool_call_limit: None,
        };
        ToolExecutionOutput::ask_user(
            serde_json::json!({"ok":true,"awaiting_user_input":true,"question":question})
                .to_string(),
            request,
        )
    }
    fn execute(&self, _: &str, _: &Value) -> String {
        unreachable!("ask_user uses execute_with_outcome")
    }
    fn execution_resource(&self, _: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Exclusive
    }
}

/// Drops blank and repeated candidate answers while preserving the model's order.
fn normalize_options(options: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut normalized: Vec<String> = Vec::new();
    for option in options.unwrap_or_default() {
        let option = option.trim().to_string();
        if option.is_empty() || normalized.contains(&option) {
            continue;
        }
        normalized.push(option);
    }
    (!normalized.is_empty()).then_some(normalized)
}
