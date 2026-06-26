use std::sync::Arc;

use log::info;
use serde_json::Value;

use zihuan_agent::brain::{Brain, BrainTool};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{LLMMessage, MessageRole};
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::task_context::append_current_task_progress;
use zihuan_core::tool_runtime::ToolRunDuration;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;

use super::agent_memory::{AgentMemoryToolResources, SearchMemoryContentBrainTool};
use super::common::{optional_string_argument, StaticFunctionToolSpec, ToolNotificationTarget};
use super::image_understand::ImageUnderstandBrainTool;
use super::web_search::WebSearchBrainTool;
use crate::agent::qq_chat::tool_quota::{wrap_brain_tool_with_quota, QqChatToolQuotaContext};

const LOG_PREFIX: &str = "[DeepResearch]";

const DEEP_RESEARCH_SYSTEM_PROMPT: &str = "\
You are a rigorous research assistant, proficient in mathematics, physics, computer science, software engineering, programming, and formal logical analysis.\n\
\n\
Your task is to conduct in-depth research on the user's question and proactively invoke tools when factual verification, web sources, or image context are needed.\n\
\n\
Research workflow requirements:\n\
1. First determine which parts can be answered directly from existing knowledge and which parts genuinely require online verification.\n\
2. If the missing information is the user's historical preferences, previously mentioned facts, or saved materials, prefer calling `search_memory_content` first; do not escalate to web research automatically.\n\
3. Use `web_search` mainly for the latest information, reading web pages, and verifying truthfulness/accuracy; do not treat it as a general background knowledge supplement.\n\
4. If the question contains both inferable parts and parts that need verification, reserve web search only for the most critical and uncertain external fact.\n\
5. Try to call `web_search` only once during a single research session; after one search, synthesize and answer first, and do not automatically continue to a second or third search.\n\
6. If the first search is still insufficient, clearly state what information is missing or cannot be confirmed, and ask the user whether to continue searching.\n\
\n\
Tool usage rules:\n\
1. `search_memory_content` retrieves long-term memories accessible in the current context, previously saved facts, historical preferences, and known materials.\n\
2. `web_search` is for online searches of the latest materials, verifying key external facts, or extracting the main text of a single URL.\n\
3. `image_understand` is used to understand image content via `media_id`.\n\
4. Before each tool call, the assistant's content must include a short progress update; this sentence will be sent directly to the user, so do not include @ and do not leave it blank. Examples: `Let me check if this was recorded before`, `Let me verify this claim`, `Let me look at the image content first`.\n\
5. After the tool returns, continue the analysis based on the tool result first; do not keep searching online just to make a directly completable analysis more complete.\n\
6. If search is necessary, keep it narrowly focused; do not mix other points into the current query, and do not treat multi-round search as the default process.\n\
\n\
Output requirements:\n\
- Problem Overview\n\
- Research Points Breakdown\n\
- Step-by-Step Analysis\n\
- Errors / Weaknesses Identified\n\
- Corrected Reasoning or Improved Solution\n\
- Final Conclusion\n\
- Important Notes and Caveats\n\
\n\
If information is insufficient, clearly state the missing information and what cannot be confirmed.";

pub(crate) struct RunDeepResearchSubagentBrainTool {
    llm: Arc<dyn LLMBase>,
    web_search_engine: Arc<WebSearchEngineRef>,
    rdb_pool: Option<RelationalDbConnection>,
    s3_ref: Option<Arc<S3Ref>>,
    weaviate_ref: Option<Arc<WeaviateRef>>,
    current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
    notification_target: ToolNotificationTarget,
    memory_resources: Option<AgentMemoryToolResources>,
    tool_quota: Option<QqChatToolQuotaContext>,
}

impl RunDeepResearchSubagentBrainTool {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        llm: Arc<dyn LLMBase>,
        web_search_engine: Arc<WebSearchEngineRef>,
        rdb_pool: Option<RelationalDbConnection>,
        s3_ref: Option<Arc<S3Ref>>,
        weaviate_ref: Option<Arc<WeaviateRef>>,
        current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
        notification_target: ToolNotificationTarget,
        memory_resources: Option<AgentMemoryToolResources>,
        tool_quota: Option<QqChatToolQuotaContext>,
    ) -> Self {
        Self {
            llm,
            web_search_engine,
            rdb_pool,
            s3_ref,
            weaviate_ref,
            current_message_event,
            notification_target,
            memory_resources,
            tool_quota,
        }
    }
}

impl BrainTool for RunDeepResearchSubagentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "run_deep_research_subagent",
            description:
                "调用深度研究子代理sub_agent，拥有更强大的分析能力。会对按研究点逐个推进，最终输出结构化的研究结论。",
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
            let progress_msg = format!("我将开始深度研究这个问题: \"{}\"", truncate_for_progress(&problem, 200));
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
                LLMMessage::system(DEEP_RESEARCH_SYSTEM_PROMPT.to_string()),
                LLMMessage::user(user_prompt),
            ];

            // Build the inner Brain with research tools.
            // Internal tools use the stored notification target (typically
            // dashboard-only) to keep internal tool progress out of QQ chat
            // while still surfacing it in the task dashboard.
            let mut brain = Brain::new(Arc::clone(&self.llm));
            if let Some(memory_resources) = self.memory_resources.clone() {
                brain.add_tool(SearchMemoryContentBrainTool::new(memory_resources));
            }
            brain.add_tool(wrap_brain_tool_with_quota(
                WebSearchBrainTool::new(Arc::clone(&self.web_search_engine), self.notification_target.clone()),
                self.tool_quota.clone(),
            ));
            brain.add_tool(ImageUnderstandBrainTool::new(
                self.current_message_event.clone(),
                self.rdb_pool.clone(),
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

            info!("{LOG_PREFIX} deep research completed, answer length: {}", trimmed.len());
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
