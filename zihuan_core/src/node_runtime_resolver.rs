use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tokio::time::timeout;

use crate::error::{Error, Result};
use crate::node_runtime::{NodeRuntimeConfig, NodeRuntimeKind};

const RUNTIME_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct NodeRuntimeCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl NodeRuntimeCommand {
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn resolve_node_runtime(workspace_root: &Path, config: &NodeRuntimeConfig) -> Result<NodeRuntimeCommand> {
    match config.kind {
        NodeRuntimeKind::ProjectNode => {
            let package = workspace_root.join("dynamic_script_engine").join("package.json");
            if !package.is_file() {
                return Err(Error::ValidationError(format!("未检测到动态脚本运行时项目: {}", package.display())));
            }
            Ok(NodeRuntimeCommand { program: PathBuf::from("node"), args: Vec::new() })
        }
        NodeRuntimeKind::CustomExecutable => {
            let raw = config
                .executable_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::ValidationError("动态脚本运行时的 Node.js 可执行文件路径不能为空".to_string()))?;
            let executable = resolve_workspace_path(workspace_root, raw);
            if !executable.is_file() {
                return Err(Error::ValidationError(format!("动态脚本运行时的 Node.js 可执行文件不存在: {}", executable.display())));
            }
            Ok(NodeRuntimeCommand { program: executable, args: Vec::new() })
        }
    }
}

pub async fn check_node_runtime(
    workspace_root: &Path,
    config: &NodeRuntimeConfig,
) -> Result<(NodeRuntimeCommand, String, String)> {
    let command_spec = resolve_node_runtime(workspace_root, config)?;
    let mut command = command_spec.to_command();
    command.arg("--version").current_dir(workspace_root.join("dynamic_script_engine"));
    let output = timeout(RUNTIME_CHECK_TIMEOUT, tokio::process::Command::from(command).output())
        .await
        .map_err(|_| Error::ValidationError("动态脚本运行时检测超时（10 秒）".to_string()))?
        .map_err(|error| Error::ValidationError(format!("无法启动动态脚本运行时: {error}")))?;
    if !output.status.success() {
        return Err(Error::ValidationError(format!(
            "动态脚本运行时检测失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok((
        command_spec.clone(),
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
        command_spec.program.display().to_string(),
    ))
}

fn resolve_workspace_path(workspace_root: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() { path } else { workspace_root.join(path) }
}
