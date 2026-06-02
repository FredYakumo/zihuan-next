use std::sync::Arc;

use log::info;
use serde_json::Value;

use zihuan_agent::brain::{Brain, BrainTool};
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::task_context::append_current_task_progress;
use zihuan_core::tool_runtime::ToolRunDuration;
use zihuan_graph_engine::object_storage::S3Ref;

use super::common::{optional_string_argument, StaticFunctionToolSpec, ToolNotificationTarget};
use super::current_time::CurrentTimeBrainTool;
use super::deep_research::RunDeepResearchSubagentBrainTool;

const LOG_PREFIX: &str = "[ResearchSubagent]";

const RESEARCH_SYSTEM_PROMPT: &str = "\
你是一个通用复杂问题处理子代理。你的任务是分析并解决复杂的推理、分析、数学、编程或其他需要深入思考的问题。\n\
\n\
处理策略：\n\
1. 首先评估问题的复杂度和信息需求。\n\
2. 如果问题可以凭你的知识直接回答，请直接给出准确、完整、可用的结果。\n\
3. 当问题需要多步骤联网搜索、资料查证、图片分析或深度交叉验证时，调用 `run_deep_research_subagent` 进行深度研究。\n\
\n\
输出要求：\n\
- 使用中文输出\n\
- 结构清晰，逻辑严密\n\
- 如果调用了深度研究，最终整合结果并给出完整结论";

pub(crate) struct RunResearchSubagentBrainTool {
    llm: Arc<dyn LLMBase>,
    web_search_engine: Arc<WebSearchEngineRef>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
    current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
    notification_target: ToolNotificationTarget,
}

impl RunResearchSubagentBrainTool {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        llm: Arc<dyn LLMBase>,
        web_search_engine: Arc<WebSearchEngineRef>,
        mysql_ref: Option<Arc<MySqlConfig>>,
        s3_ref: Option<Arc<S3Ref>>,
        current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            llm,
            web_search_engine,
            mysql_ref,
            s3_ref,
            current_message_event,
            notification_target,
        }
    }
}

impl BrainTool for RunResearchSubagentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "run_research_subagent",
            description: "调用专家subagent处理复杂问题，编程、数学、分析推理等复杂问题都可以调用它来得出更准确和可靠的结论",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "problem": {
                        "type": "string",
                        "description": "要交给研究sub_agent处理的复杂问题"
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

    fn run_duration(&self) -> ToolRunDuration {
        ToolRunDuration::Long
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let problem = optional_string_argument(arguments, "problem")
                .ok_or_else(|| Error::ValidationError("problem is required".to_string()))?;
            let context = optional_string_argument(arguments, "context");
            let output_requirements = optional_string_argument(arguments, "output_requirements");

            // Write initial task progress so the dashboard shows research has started.
            let progress_msg = format!(
                "我将开始处理这个问题: \"{}\"",
                truncate_for_progress(&problem, 200)
            );
            append_current_task_progress(progress_msg);

            let mut user_prompt = problem.clone();
            if let Some(ctx) = &context {
                user_prompt.push_str(&format!("\n\n补充上下文：\n{ctx}"));
            }
            if let Some(reqs) = &output_requirements {
                user_prompt.push_str(&format!("\n\n输出要求：\n{reqs}"));
            }

            info!(
                "{LOG_PREFIX} starting research, problem preview: {}",
                truncate_for_progress(&problem, 200)
            );

            let messages = vec![
                OpenAIMessage::system(RESEARCH_SYSTEM_PROMPT.to_string()),
                OpenAIMessage::user(user_prompt),
            ];

            // Build inner Brain with deep_research as the escalation tool.
            // The subagent evaluates the problem and either answers directly
            // or escalates to deep_research for multi-step web research.
            let mut brain = Brain::new(Arc::clone(&self.llm));
            brain.add_tool(CurrentTimeBrainTool);
            brain.add_tool(RunDeepResearchSubagentBrainTool::new(
                Arc::clone(&self.llm),
                Arc::clone(&self.web_search_engine),
                self.mysql_ref.clone(),
                self.s3_ref.clone(),
                self.current_message_event.clone(),
                self.notification_target.clone(),
            ));

            let (output_messages, _stop_reason) = brain.run(messages);

            let answer = output_messages
                .iter()
                .rev()
                .find(|msg| matches!(msg.role, MessageRole::Assistant))
                .and_then(|msg| msg.content_text_owned())
                .unwrap_or_default();

            let trimmed = answer.trim().to_string();
            if trimmed.is_empty() {
                return Err(Error::ValidationError(
                    "research subagent returned empty response".to_string(),
                ));
            }

            info!(
                "{LOG_PREFIX} research completed, answer length: {}",
                trimmed.len()
            );
            Ok(trimmed)
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

fn truncate_for_progress(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...")
}
