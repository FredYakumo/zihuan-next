use std::path::PathBuf;
use std::sync::Arc;

use model_inference::system_config::{AgentConfig, WorkspaceAgentServiceConfig};
use storage_handler::ConnectionConfig;
use zihuan_agent::brain::BrainTool;
use zihuan_core::llm::LLMMessage;
use zihuan_core::workspace::normalized_workspace_path;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;

use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::tool_definitions::build_enabled_tool_definitions;
use super::tools::{
    AskUserBrainTool, CreateFileBrainTool, DeleteFileBrainTool, EditFileBrainTool, ExecCmdBrainTool,
    CopyFileBrainTool, FileInfoBrainTool, FindFilesBrainTool, GitStatusBrainTool, GrepBrainTool, ListDirBrainTool, MoveFileBrainTool, ReadFileBrainTool, RgBrainTool, DEFAULT_TOOL_ASK_USER,
    DEFAULT_TOOL_CREATE_FILE, DEFAULT_TOOL_DELETE_FILE, DEFAULT_TOOL_EDIT_FILE, DEFAULT_TOOL_EXEC_CMD,
    DEFAULT_TOOL_COPY_FILE, DEFAULT_TOOL_FILE_INFO, DEFAULT_TOOL_FIND_FILES, DEFAULT_TOOL_GIT_STATUS, DEFAULT_TOOL_GREP, DEFAULT_TOOL_LIST_DIR, DEFAULT_TOOL_MOVE_FILE, DEFAULT_TOOL_READ_FILE, DEFAULT_TOOL_RG,
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
                    "当前工作目录是: {path}\n你可以在该目录下读取文件、列出目录、按名称查找路径、搜索文本、创建、编辑、删除、复制、移动文件，查看元数据和 Git 状态，以及执行命令。read_file 可在需要时使用 base64 读取二进制片段。"
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
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_READ_FILE) {
            tools.push(Box::new(ReadFileBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_LIST_DIR) {
            tools.push(Box::new(ListDirBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GREP) {
            tools.push(Box::new(GrepBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_RG) {
            tools.push(Box::new(RgBrainTool {
                workspace_path: workspace_path.clone(),
            }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FIND_FILES) {
            tools.push(Box::new(FindFilesBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_COPY_FILE) {
            tools.push(Box::new(CopyFileBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_MOVE_FILE) {
            tools.push(Box::new(MoveFileBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_FILE_INFO) {
            tools.push(Box::new(FileInfoBrainTool { workspace_path: workspace_path.clone() }));
        }
        if is_enabled(&self.default_tools_enabled, DEFAULT_TOOL_GIT_STATUS) {
            tools.push(Box::new(GitStatusBrainTool { workspace_path: workspace_path.clone() }));
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
    config: &WorkspaceAgentServiceConfig,
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
