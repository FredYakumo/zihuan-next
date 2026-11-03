use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::agent::declarative_agent::{
    AgentDefinition, AgentHost, AgentOutputMode,
};
use crate::error::Result;
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::DataType;
use crate::system_config::current_memory_agent_prompts;

use super::full_search_concept_memory::FullSearchConceptMemory;
use super::recall::MemoryRecall;
use super::resources::MemoryAgentResources;

pub const MEMORY_AGENT_ID: &str = "memory_agent";
const MEMORY_AGENT_NAME: &str = "Memory";

fn memory_string_port(name: &str, description: &str, required: bool) -> FunctionPortDef {
    FunctionPortDef {
        name: name.to_string(),
        data_type: DataType::String,
        description: description.to_string(),
        required,
    }
}

/// Builds the memory agent definition from the configurable prompts. This replaces the
/// former built-in `memory_agent.yaml`; the Procedure owns recall and exposes only full search.
pub fn memory_agent_definition(
    prompts: &crate::system_config::MemoryAgentPrompts,
) -> AgentDefinition {
    AgentDefinition {
        id: MEMORY_AGENT_ID.to_string(),
        name: MEMORY_AGENT_NAME.to_string(),
        description: "Recall information relevant to the current user input".to_string(),
        builtin: true,
        inputs: vec![memory_string_port("content", "Current input with verified sender QQ ID", true)],
        outputs: vec![memory_string_port("result", "Recall summary", true)],
        system_prompt: format!("{}\nYou are the Memory Agent Procedure phase. Only recall, never write. Search relevant facts, people and behavior using FullSearchConceptMemory. QQ IDs identify people; never infer shared identity from aliases. When the user challenges a recollection use force_check=true. Query the current sender's QQ ID as well as topics. Do not invent memories.", prompts.system_prompt),
        user_prompt: Some("{content}".to_string()),
        prompt_parts: vec![],
        output_mode: AgentOutputMode::Text,
        llm_kind: crate::agent::LLM_KIND_MAIN.to_string(),
        progress_message: None,
        include_graph_tools: false,
        run_duration: crate::tool_runtime::ToolRunDuration::default(),
        tool_ids: vec!["FullSearchConceptMemory".to_string()],
    }
}

/// Runs the recall phase: a small agent with only the full-search tool, whose accepted hits are
/// collected typed in [`MemoryRecall`].
pub fn recall_memory(
    resources: MemoryAgentResources,
    input: &str,
    previous_context: String,
) -> Result<MemoryRecall> {
    let results = Arc::new(Mutex::new(MemoryRecall::default()));
    let mut host = AgentHost::new();
    host.register_llm(crate::agent::LLM_KIND_MAIN, resources.llm.clone());
    host.register_tool(
        "FullSearchConceptMemory",
        Arc::new(FullSearchConceptMemory::new(
            resources.clone(),
            previous_context,
            results.clone(),
        )),
    );
    let definition = memory_agent_definition(&current_memory_agent_prompts());
    let agent = host.build_definition(definition)?;
    let mut ports = HashMap::new();
    ports.insert("content".to_string(), crate::graph::DataValue::String(input.to_string()));
    // Recall the verified sender and applicable instructions even when the phase model skips tools.
    let full = host.tool("FullSearchConceptMemory").unwrap();
    if let Some(sender) = &resources.access.sender_id {
        full.execute("", &json!({"query":sender}))?;
    }
    let _ = agent.run_text(ports)?;
    let recalled = (*results.lock().unwrap()).clone();
    Ok(recalled)
}
