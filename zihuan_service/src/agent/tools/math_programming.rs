use std::sync::Arc;

use serde_json::Value;
use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{InferenceParam, OpenAIMessage};

use super::common::{optional_string_argument, StaticFunctionToolSpec};

pub(crate) struct RunMathProgrammingSubagentBrainTool {
    llm: Arc<dyn LLMBase>,
}

impl RunMathProgrammingSubagentBrainTool {
    pub(crate) fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self { llm }
    }
}

impl BrainTool for RunMathProgrammingSubagentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "run_math_programming_subagent",
            description: "调用数学/编程专用子代理处理复杂推理、代码或数学问题，并返回结果文本。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "problem": {
                        "type": "string",
                        "description": "要交给数学/编程子代理处理的问题"
                    },
                    "context": {
                        "type": "string",
                        "description": "可选：补充上下文"
                    },
                    "output_requirements": {
                        "type": "string",
                        "description": "可选：输出要求，例如给出代码、只给结论、分步解释等"
                    }
                },
                "required": ["problem"],
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let problem = optional_string_argument(arguments, "problem")
                .ok_or_else(|| Error::ValidationError("problem is required".to_string()))?;
            let context = optional_string_argument(arguments, "context");
            let output_requirements = optional_string_argument(arguments, "output_requirements");

            let mut user_prompt = format!("请处理下面的问题：\n{problem}");
            if let Some(context) = context {
                user_prompt.push_str(&format!("\n\n补充上下文：\n{context}"));
            }
            if let Some(output_requirements) = output_requirements {
                user_prompt.push_str(&format!("\n\n输出要求：\n{output_requirements}"));
            }

            let messages = vec![
                OpenAIMessage::system(
                    "你是一个数学与编程专用子代理。请专注完成推理、分析、代码实现或数学求解，输出准确、完整、直接可用的结果。"
                        .to_string(),
                ),
                OpenAIMessage::user(user_prompt),
            ];
            let response = self.llm.inference(&InferenceParam {
                messages: &messages,
                tools: None,
            });
            let content = response.content_text_owned().unwrap_or_default();
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return Err(Error::ValidationError(
                    "math programming subagent returned empty response".to_string(),
                ));
            }
            Ok(trimmed.to_string())
        })();

        match result {
            Ok(message) => message,
            Err(error) => serde_json::json!({
                "ok": false,
                "error": error.to_string(),
            })
            .to_string(),
        }
    }
}
