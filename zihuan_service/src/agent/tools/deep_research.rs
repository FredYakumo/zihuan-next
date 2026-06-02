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
use super::image_understand::ImageUnderstandBrainTool;
use super::web_search::WebSearchBrainTool;

const LOG_PREFIX: &str = "[DeepResearch]";

const DEEP_RESEARCH_SYSTEM_PROMPT: &str = "\
你是一个严谨的研究助理，擅长数学、物理、计算机科学、软件工程、编程与形式逻辑分析。\n\
\n\
你的任务是对用户问题做深入研究，并在需要事实查证、网页资料或图片上下文时主动调用工具。\n\
\n\
研究流程要求：\n\
1. 先把用户问题拆成若干个彼此独立的研究点、子问题或核查点。\n\
2. 必须按\"一个研究点一轮处理\"的方式推进：每一轮只围绕一个研究点进行检索、阅读、核查和整理。\n\
3. 禁止把多个研究点混在同一次 `web_search` 调用里；如果有多个点，就分成多次工具调用逐个处理。\n\
4. 只有在当前研究点已有足够证据后，才能进入下一个研究点。\n\
5. 在所有研究点处理完成后，再做统一汇总、交叉校验和最终结论。\n\
\n\
工具使用规则：\n\
1. `web_search` 用于联网搜索资料，或对单个 URL 抽取网页正文。\n\
2. `image_understand` 用于通过 `media_id` 理解图片内容。\n\
3. 每次调用工具前，assistant 的 content 必须先写一句简短中文进度提示；这句话会直接回给用户，所以不要包含 @，也不要留空。例如：`我先查第一个点`、`我先核对这个说法`、`我先看看图片内容`。\n\
4. 工具返回后再继续分析，优先基于工具结果作答。\n\
5. 如果某个研究点需要多次搜索，也要保持单点聚焦，不要把别的点混入当前搜索词。\n\
\n\
输出要求：\n\
- Problem Overview\n\
- Research Points Breakdown\n\
- Step-by-Step Analysis\n\
- Errors / Weaknesses Identified\n\
- Corrected Reasoning or Improved Solution\n\
- Final Conclusion\n\
- Important Notes and Caveats\n\
\n\
如果信息不足，要明确说明缺失信息和无法确认的部分。最终回答使用中文。";

pub(crate) struct RunDeepResearchSubagentBrainTool {
    llm: Arc<dyn LLMBase>,
    web_search_engine: Arc<WebSearchEngineRef>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
    current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
    notification_target: ToolNotificationTarget,
}

impl RunDeepResearchSubagentBrainTool {
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

impl BrainTool for RunDeepResearchSubagentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "run_deep_research_subagent",
            description: "调用深度研究子代理sub_agent，拥有更强大的分析能力。会对按研究点逐个推进，最终输出结构化的研究结论。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "problem": {
                        "type": "string",
                        "description": "要交给深度研究subagent处理的问题"
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
                "我将开始深度研究这个问题: \"{}\"",
                truncate_for_progress(&problem, 200)
            );
            append_current_task_progress(progress_msg);

            // Build the user message from problem + optional context and output requirements.
            let mut user_prompt = problem.clone();
            if let Some(ctx) = &context {
                user_prompt.push_str(&format!("\n\n补充上下文：\n{ctx}"));
            }
            if let Some(reqs) = &output_requirements {
                user_prompt.push_str(&format!("\n\n输出要求：\n{reqs}"));
            }

            info!(
                "{LOG_PREFIX} starting deep research, problem preview: {}",
                truncate_for_progress(&problem, 200)
            );

            let messages = vec![
                OpenAIMessage::system(DEEP_RESEARCH_SYSTEM_PROMPT.to_string()),
                OpenAIMessage::user(user_prompt),
            ];

            // Build the inner Brain with research tools.
            // Internal tools use the stored notification target (typically
            // dashboard-only) to keep internal tool progress out of QQ chat
            // while still surfacing it in the task dashboard.
            let mut brain = Brain::new(Arc::clone(&self.llm));
            brain.add_tool(CurrentTimeBrainTool);
            brain.add_tool(WebSearchBrainTool::new(
                Arc::clone(&self.web_search_engine),
                self.notification_target.clone(),
            ));
            brain.add_tool(ImageUnderstandBrainTool::new(
                self.current_message_event.clone(),
                self.mysql_ref.clone(),
                self.s3_ref.clone(),
                self.notification_target.clone(),
            ));

            // Run the full research loop (multi-turn, tool-calling).
            let (output_messages, _stop_reason) = brain.run(messages);

            // Extract the final assistant message content as the research answer.
            let answer = output_messages
                .iter()
                .rev()
                .find(|msg| matches!(msg.role, MessageRole::Assistant))
                .and_then(|msg| msg.content_text_owned())
                .unwrap_or_default();

            let trimmed = answer.trim().to_string();
            if trimmed.is_empty() {
                return Err(Error::ValidationError(
                    "deep research subagent returned empty response".to_string(),
                ));
            }

            info!(
                "{LOG_PREFIX} deep research completed, answer length: {}",
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
