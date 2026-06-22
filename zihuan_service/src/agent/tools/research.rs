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
use zihuan_graph_engine::object_storage::S3Ref;

use super::agent_memory::AgentMemoryToolResources;
use super::common::{optional_string_argument, StaticFunctionToolSpec, ToolNotificationTarget};
use super::current_time::CurrentTimeBrainTool;
use super::deep_research::RunDeepResearchSubagentBrainTool;
use crate::agent::qq_chat::tool_quota::{wrap_brain_tool_with_quota, QqChatToolQuotaContext};

const LOG_PREFIX: &str = "[ResearchSubagent]";

const RESEARCH_SYSTEM_PROMPT: &str = "\
    You are a general-purpose complex-problem sub-agent. Your job is to analyze and solve difficult reasoning, analytical, mathematical, programming, or other deep-thinking problems.\n\
    \n\
    Strategy:\n\
    1. First assess the problem's complexity and information needs.\n\
    2. If the problem can be solved directly with your knowledge, reasoning, and general experience, provide an accurate, complete, and actionable answer.\n\
    3. For math, programming, logic analysis, and solution-design problems, default to offline reasoning; do not assume the web is required just because the problem looks hard.\n\
    4. If the only gap is user context, historical preferences, or previously mentioned details, ask the main agent to use `search_memory_content` rather than escalating uncertainty into web research.\n\
    5. Only call `run_deep_research_subagent` when the problem genuinely depends on the latest external sources, web content, or fact-checking and cannot be completed with existing knowledge alone.\n\
    \n\
    Output requirements:\n\
    - Respond in Chinese.\n\
    - Structure must be clear and logic rigorous.\n\
    - If deep research is invoked, synthesize the results and present a complete conclusion.";

pub(crate) struct RunResearchSubagentBrainTool {
    llm: Arc<dyn LLMBase>,
    web_search_engine: Arc<WebSearchEngineRef>,
    rdb_pool: Option<RelationalDbConnection>,
    s3_ref: Option<Arc<S3Ref>>,
    current_message_event: Option<ims_bot_adapter::models::MessageEvent>,
    notification_target: ToolNotificationTarget,
    memory_resources: Option<AgentMemoryToolResources>,
    tool_quota: Option<QqChatToolQuotaContext>,
}

impl RunResearchSubagentBrainTool {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        llm: Arc<dyn LLMBase>,
        web_search_engine: Arc<WebSearchEngineRef>,
        rdb_pool: Option<RelationalDbConnection>,
        s3_ref: Option<Arc<S3Ref>>,
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
            current_message_event,
            notification_target,
            memory_resources,
            tool_quota,
        }
    }
}

impl BrainTool for RunResearchSubagentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "run_research_subagent",
            description: "Invoke an expert sub-agent to handle complex problems such as programming, mathematics, and analytical reasoning for more accurate and reliable conclusions.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "problem": {
                        "type": "string",
                        "description": "The complex problem to be handled by the research sub-agent."
                    },
                    "context": {
                        "type": "string",
                        "description": "Optional: supplementary context."
                    },
                    "output_requirements": {
                        "type": "string",
                        "description": "Optional: output requirements, e.g., provide code, conclusion only, step-by-step explanation, etc."
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
                "I will start working on this problem: \"{}\"",
                truncate_for_progress(&problem, 200)
            );
            append_current_task_progress(progress_msg);

            let mut user_prompt = problem.clone();
            if let Some(ctx) = &context {
                user_prompt.push_str(&format!("\n\nSupplementary context:\n{ctx}"));
            }
            if let Some(reqs) = &output_requirements {
                user_prompt.push_str(&format!("\n\nOutput requirements:\n{reqs}"));
            }

            info!(
                "{LOG_PREFIX} starting research, problem preview: {}",
                truncate_for_progress(&problem, 200)
            );

            let messages = vec![
                LLMMessage::system(RESEARCH_SYSTEM_PROMPT.to_string()),
                LLMMessage::user(user_prompt),
            ];

            // Build inner Brain with deep_research as the escalation tool.
            // The subagent evaluates the problem and either answers directly
            // or escalates to deep_research for multi-step web research.
            let mut brain = Brain::new(Arc::clone(&self.llm));
            brain.add_tool(CurrentTimeBrainTool);
            brain.add_tool(wrap_brain_tool_with_quota(
                RunDeepResearchSubagentBrainTool::new(
                    Arc::clone(&self.llm),
                    Arc::clone(&self.web_search_engine),
                    self.rdb_pool.clone(),
                    self.s3_ref.clone(),
                    self.current_message_event.clone(),
                    self.notification_target.clone(),
                    self.memory_resources.clone(),
                    self.tool_quota.clone(),
                ),
                self.tool_quota.clone(),
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
                return Err(Error::ValidationError("research subagent returned empty response".to_string()));
            }

            info!("{LOG_PREFIX} research completed, answer length: {}", trimmed.len());
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
