use std::path::PathBuf;
use std::sync::Arc;

use model_inference::system_config::{AgentConfig, WorkspaceAgentConfig};
use storage_handler::ConnectionConfig;
use zihuan_agent::brain::BrainTool;
use zihuan_core::llm::LLMMessage;
use zihuan_core::workspace::normalized_workspace_path;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;

use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::tool_definitions::build_enabled_tool_definitions;
use super::tools::{
    AskUserBrainTool, CreateFileBrainTool, DeleteFileBrainTool, EditFileBrainTool, ExecCmdBrainTool,
    DEFAULT_TOOL_ASK_USER, DEFAULT_TOOL_CREATE_FILE, DEFAULT_TOOL_DELETE_FILE, DEFAULT_TOOL_EDIT_FILE,
    DEFAULT_TOOL_EXEC_CMD,
};
use zihuan_core::error::Result;

pub struct WorkspaceInferenceToolProvider {
    default_tools_enabled: std::collections::HashMap<String, bool>,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl InferenceToolProvider for WorkspaceInferenceToolProvider {
    fn augment_messages(&self, messages: &mut Vec<LLMMessage>, context: &InferenceToolContext) {
        if let Some(ref path) = context.workspace_path {
            messages.insert(
                0,
                LLMMessage::system(format!(
                    "当前工作目录是: {path}\n你可以在该目录下创建、编辑、删除文件，以及执行命令。"
                )),
            );
        }
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        let workspace_path = normalized_workspace_path(context.workspace_path.as_deref()).map(PathBuf::from);
        let mut tools: Vec<Box<dyn BrainTool>> = Vec::new();
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_CREATE_FILE) {
            tools.push(Box::new(CreateFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_DELETE_FILE) {
            tools.push(Box::new(DeleteFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EDIT_FILE) {
            tools.push(Box::new(EditFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_EXEC_CMD) {
            tools.push(Box::new(ExecCmdBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_ASK_USER) {
            tools.push(Box::new(AskUserBrainTool));
        }
        tools
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &AgentConfig,
    config: &WorkspaceAgentConfig,
    _connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(WorkspaceInferenceToolProvider {
        default_tools_enabled: config.default_tools_enabled.clone(),
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn is_enabled(map: &std::collections::HashMap<String, bool>, name: &str) -> bool {
    *map.get(name).unwrap_or(&true)
}
