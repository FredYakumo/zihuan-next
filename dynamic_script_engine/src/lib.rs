use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::time::timeout;

pub type Result<T> = std::result::Result<T, EngineError>;
pub type HostHandler<'a> = dyn FnMut(&str, &Value) -> std::result::Result<Value, String> + 'a;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

fn message(value: impl Into<String>) -> EngineError { EngineError::Message(value.into()) }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLanguage { JavaScript, Python }

impl ScriptLanguage {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|value| value.to_str()) {
            Some("mjs") => Some(Self::JavaScript),
            Some("py") => Some(Self::Python),
            _ => None,
        }
    }

    fn runner_file(self) -> &'static str {
        match self { Self::JavaScript => "engine.mjs", Self::Python => "python_node_runtime.py" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptCatalog {
    pub nodes: Vec<Value>,
    #[serde(default)]
    pub diagnostics: Vec<ScriptDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptDiagnostic {
    pub language: ScriptLanguage,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PythonRuntimeKind { #[default] UvProject, #[serde(alias = "venv_python")] ProjectVenv, CustomExecutable }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PythonRuntimeConfig {
    #[serde(default)] pub kind: PythonRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub executable_path: Option<String>,
}
impl Default for PythonRuntimeConfig { fn default() -> Self { Self { kind: PythonRuntimeKind::UvProject, executable_path: None } } }
impl From<PythonRuntimeKind> for PythonRuntimeConfig { fn from(kind: PythonRuntimeKind) -> Self { Self { kind, executable_path: None } } }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeRuntimeKind { #[default] ProjectNode, CustomExecutable }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRuntimeConfig {
    #[serde(default)] pub kind: NodeRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub executable_path: Option<String>,
}
impl Default for NodeRuntimeConfig { fn default() -> Self { Self { kind: NodeRuntimeKind::ProjectNode, executable_path: None } } }
impl From<NodeRuntimeKind> for NodeRuntimeConfig { fn from(kind: NodeRuntimeKind) -> Self { Self { kind, executable_path: None } } }

#[derive(Debug, Clone)]
pub struct RuntimeCommand { pub program: PathBuf, pub args: Vec<String> }
impl RuntimeCommand {
    pub fn to_command(&self) -> Command { let mut command = Command::new(&self.program); command.args(&self.args); command }
    pub fn display(&self) -> String { std::iter::once(self.program.display().to_string()).chain(self.args.iter().cloned()).collect::<Vec<_>>().join(" ") }
}

pub fn resolve_node_runtime(workspace: &Path, config: &NodeRuntimeConfig) -> Result<RuntimeCommand> {
    match config.kind {
        NodeRuntimeKind::ProjectNode => {
            let package = engine_dir(workspace).join("package.json");
            if !package.is_file() { return Err(message(format!("未检测到动态脚本运行时项目: {}", package.display()))); }
            Ok(RuntimeCommand { program: PathBuf::from("node"), args: Vec::new() })
        }
        NodeRuntimeKind::CustomExecutable => Ok(RuntimeCommand { program: custom_executable(workspace, config.executable_path.as_deref(), "动态脚本运行时的 Node.js 可执行文件")?, args: Vec::new() }),
    }
}

pub fn resolve_python_runtime(workspace: &Path, config: &PythonRuntimeConfig) -> Result<RuntimeCommand> {
    match config.kind {
        PythonRuntimeKind::UvProject => Ok(RuntimeCommand { program: PathBuf::from("uv"), args: vec!["run".to_string(), "python".to_string()] }),
        PythonRuntimeKind::ProjectVenv => {
            let executable = project_venv_python_path(workspace);
            if !executable.is_file() { return Err(message(format!("项目 Python 虚拟环境不存在: {}", executable.display()))); }
            Ok(RuntimeCommand { program: executable, args: Vec::new() })
        }
        PythonRuntimeKind::CustomExecutable => Ok(RuntimeCommand { program: custom_executable(workspace, config.executable_path.as_deref(), "自定义 Python 解释器")?, args: Vec::new() }),
    }
}

pub fn project_venv_python_path(workspace: &Path) -> PathBuf { if cfg!(windows) { workspace.join(".venv/Scripts/python.exe") } else { workspace.join(".venv/bin/python") } }
fn engine_dir(workspace: &Path) -> PathBuf { workspace.join("dynamic_script_engine") }
fn custom_executable(workspace: &Path, raw: Option<&str>, label: &str) -> Result<PathBuf> {
    let raw = raw.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| message(format!("{label}路径不能为空")))?;
    let path = PathBuf::from(raw); let path = if path.is_absolute() { path } else { workspace.join(path) };
    if !path.is_file() { return Err(message(format!("{label}不存在: {}", path.display()))); } Ok(path)
}

pub async fn check_node_runtime(workspace: &Path, config: &NodeRuntimeConfig) -> Result<(RuntimeCommand, String, String)> { check_runtime(resolve_node_runtime(workspace, config)?, workspace, "动态脚本运行时").await }
pub async fn check_python_runtime(workspace: &Path, config: &PythonRuntimeConfig) -> Result<(RuntimeCommand, String, String)> {
    let command = resolve_python_runtime(workspace, config)?;
    if config.kind == PythonRuntimeKind::UvProject {
        let project = workspace.join("pyproject.toml"); if !project.is_file() { return Err(message(format!("未检测到 pyproject.toml: {}", project.display()))); }
        let output = timed_output({ let mut cmd = Command::new("uv"); cmd.arg("--version"); cmd }, "uv").await?;
        if !output.status.success() { return Err(command_failure("uv 运行时检测失败", &output.stderr)); }
        return Ok((command, String::from_utf8_lossy(&output.stdout).trim().to_string(), project.display().to_string()));
    }
    check_runtime(command, workspace, "Python 运行时").await
}
async fn check_runtime(command: RuntimeCommand, workspace: &Path, label: &str) -> Result<(RuntimeCommand, String, String)> {
    let mut process = command.to_command(); process.arg("--version").current_dir(workspace); let output = timed_output(process, label).await?;
    if !output.status.success() { return Err(command_failure(&format!("{label}检测失败"), &output.stderr)); }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string(); let version = if version.is_empty() { String::from_utf8_lossy(&output.stderr).trim().to_string() } else { version };
    Ok((command.clone(), version, command.program.display().to_string()))
}
async fn timed_output(command: Command, label: &str) -> Result<std::process::Output> {
    let program = command.get_program().to_owned(); let args: Vec<_> = command.get_args().map(|arg| arg.to_owned()).collect(); let directory = command.get_current_dir().map(PathBuf::from);
    let mut command = tokio::process::Command::new(program); command.args(args).stdout(Stdio::piped()).stderr(Stdio::piped()); if let Some(directory) = directory { command.current_dir(directory); }
    let mut child = command.spawn().map_err(|error| message(format!("无法启动{label}: {error}")))?;
    let status = match timeout(Duration::from_secs(10), child.wait()).await { Ok(result) => result.map_err(|error| message(format!("无法等待{label}: {error}")))?, Err(_) => { let _ = child.kill().await; return Err(message(format!("{label}检测超时（10 秒）"))); } };
    let mut stdout = Vec::new(); if let Some(mut stream) = child.stdout.take() { stream.read_to_end(&mut stdout).await.map_err(|error| message(format!("无法读取{label}输出: {error}")))?; }
    let mut stderr = Vec::new(); if let Some(mut stream) = child.stderr.take() { stream.read_to_end(&mut stderr).await.map_err(|error| message(format!("无法读取{label}错误输出: {error}")))?; }
    Ok(std::process::Output { status, stdout, stderr })
}
fn command_failure(prefix: &str, stderr: &[u8]) -> EngineError { let stderr = String::from_utf8_lossy(stderr).trim().to_string(); message(format!("{prefix}: {}", if stderr.is_empty() { "暂未检测到" } else { &stderr })) }

pub fn discover_languages(workspace: &Path) -> Result<BTreeSet<ScriptLanguage>> {
    fn visit(directory: &Path, languages: &mut BTreeSet<ScriptLanguage>) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? { let entry = entry?; let path = entry.path(); if path.is_dir() { visit(&path, languages)?; } else if let Some(language) = ScriptLanguage::from_path(&path) { languages.insert(language); } }
        Ok(())
    }
    let mut languages = BTreeSet::new(); let directory = workspace.join("dag_nodes"); if directory.is_dir() { visit(&directory, &mut languages)?; } Ok(languages)
}

pub fn load_script_catalog(workspace: &Path, node: &NodeRuntimeConfig, python: &PythonRuntimeConfig) -> Result<ScriptCatalog> {
    let mut catalog = ScriptCatalog { nodes: Vec::new(), diagnostics: Vec::new() };
    for language in discover_languages(workspace)? {
        let command = match language { ScriptLanguage::JavaScript => resolve_node_runtime(workspace, node), ScriptLanguage::Python => resolve_python_runtime(workspace, python) };
        let command = match command { Ok(command) => command, Err(error) => { catalog.diagnostics.push(ScriptDiagnostic { language, message: error.to_string() }); continue; } };
        match run_once(workspace, language, &command, "--catalog", None) {
            Ok(Value::Object(mut response)) => {
                let diagnostics: Vec<ScriptDiagnostic> = response
                    .remove("diagnostics")
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
                catalog.diagnostics.extend(diagnostics);
                let nodes = response
                    .remove("nodes")
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_default();
                for mut definition in nodes { definition.as_object_mut().ok_or_else(|| message(format!("{language:?} 节点定义必须是对象")))?.insert("language".to_string(), serde_json::to_value(language)?); catalog.nodes.push(definition); }
            }
            Ok(_) => return Err(message(format!("{language:?} 节点目录响应无效"))),
            Err(error) => catalog.diagnostics.push(ScriptDiagnostic { language, message: error.to_string() }),
        }
    }
    let mut ids = BTreeSet::new(); for definition in &catalog.nodes { let type_id = definition.get("type_id").and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| message("动态节点缺少 type_id"))?; if !ids.insert(type_id.to_string()) { return Err(message(format!("动态节点 type_id 重复: {type_id}"))); } }
    Ok(catalog)
}

struct Worker { child: Child, stdin: ChildStdin, stdout: BufReader<ChildStdout> }
impl Worker {
    fn start(workspace: &Path, language: ScriptLanguage, command: &RuntimeCommand) -> Result<Self> {
        let mut child = command.to_command().arg(engine_dir(workspace).join(language.runner_file())).arg("--serve").current_dir(engine_dir(workspace)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        Ok(Self { stdin: child.stdin.take().ok_or_else(|| message("无法打开动态脚本运行时 stdin"))?, stdout: BufReader::new(child.stdout.take().ok_or_else(|| message("无法打开动态脚本运行时 stdout"))?), child })
    }
    fn running(&mut self) -> Result<bool> { Ok(self.child.try_wait()?.is_none()) }
    fn request(&mut self, request: &Value, host: &mut HostHandler<'_>) -> Result<Value> {
        serde_json::to_writer(&mut self.stdin, request)?; self.stdin.write_all(b"\n")?; self.stdin.flush()?;
        loop { let mut line = String::new(); if self.stdout.read_line(&mut line)? == 0 { return Err(message("动态脚本运行时在响应前退出")); } let response: Value = serde_json::from_str(&line).map_err(|error| message(format!("动态脚本运行时响应无效: {error}")))?; if response.get("kind").and_then(Value::as_str) != Some("host_request") { return Ok(response); } let id = response.get("id").cloned().unwrap_or(Value::Null); let reply = match host(response.get("method").and_then(Value::as_str).unwrap_or_default(), response.get("params").unwrap_or(&Value::Null)) { Ok(result) => json!({"kind":"host_response","id":id,"result":result}), Err(error) => json!({"kind":"host_response","id":id,"error":error}) }; serde_json::to_writer(&mut self.stdin, &reply)?; self.stdin.write_all(b"\n")?; self.stdin.flush()?; }
    }
}
static WORKERS: Lazy<Mutex<HashMap<ScriptLanguage, Worker>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn start_script_runtime(workspace: &Path, language: ScriptLanguage, node: &NodeRuntimeConfig, python: &PythonRuntimeConfig) -> Result<()> {
    let command = match language { ScriptLanguage::JavaScript => resolve_node_runtime(workspace, node), ScriptLanguage::Python => resolve_python_runtime(workspace, python) }?;
    let mut workers = WORKERS.lock().map_err(|_| message("动态脚本运行时互斥锁不可用"))?;
    if workers.get_mut(&language).map(|worker| worker.running()).transpose()?.unwrap_or(false) { return Ok(()); }
    workers.insert(language, Worker::start(workspace, language, &command)?); Ok(())
}
pub fn request_script_runtime(workspace: &Path, language: ScriptLanguage, node: &NodeRuntimeConfig, python: &PythonRuntimeConfig, request: &Value, host: &mut HostHandler<'_>) -> Result<Value> {
    start_script_runtime(workspace, language, node, python)?; let mut workers = WORKERS.lock().map_err(|_| message("动态脚本运行时互斥锁不可用"))?; let result = workers.get_mut(&language).expect("worker initialized").request(request, host); if result.is_err() { workers.remove(&language); } result
}
pub fn resolve_script_ports(workspace: &Path, language: ScriptLanguage, node: &NodeRuntimeConfig, python: &PythonRuntimeConfig, request: &Value) -> Result<Value> { let command = match language { ScriptLanguage::JavaScript => resolve_node_runtime(workspace, node), ScriptLanguage::Python => resolve_python_runtime(workspace, python) }?; run_once(workspace, language, &command, "--ports", Some(request)) }
fn run_once(workspace: &Path, language: ScriptLanguage, command: &RuntimeCommand, argument: &str, request: Option<&Value>) -> Result<Value> { let mut command = command.to_command(); command.arg(engine_dir(workspace).join(language.runner_file())).arg(argument).current_dir(engine_dir(workspace)).stdout(Stdio::piped()).stderr(Stdio::piped()); if request.is_some() { command.stdin(Stdio::piped()); } let mut child = command.spawn()?; if let Some(request) = request { serde_json::to_writer(child.stdin.as_mut().ok_or_else(|| message("无法打开动态脚本运行时 stdin"))?, request)?; } let output = child.wait_with_output()?; if !output.status.success() { return Err(message(String::from_utf8_lossy(&output.stderr).trim().to_string())); } serde_json::from_slice(&output.stdout).map_err(|error| message(format!("动态脚本运行时响应不是合法 JSON: {error}"))) }

pub fn execute_python_script(workspace: &Path, config: &PythonRuntimeConfig, script_path: &Path, entry: &str, _timeout_secs: u64, request: &Value, host: &mut HostHandler<'_>) -> Result<Value> {
    if !script_path.is_file() { return Err(message(format!("Python 脚本不存在: {}", script_path.display()))); }
    let mut request = request.clone(); request.as_object_mut().ok_or_else(|| message("Python 工具请求必须是对象"))?.insert("script_path".to_string(), Value::String(script_path.display().to_string())); request.as_object_mut().expect("object").insert("entry".to_string(), Value::String(entry.to_string()));
    let response = request_script_runtime(workspace, ScriptLanguage::Python, &NodeRuntimeConfig::default(), config, &json!({"kind":"tool_execute","request":request}), host)?;
    response.get("response").cloned().ok_or_else(|| message("Python 工具响应缺少 response"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_supported_node_languages() { assert_eq!(ScriptLanguage::from_path(Path::new("node.mjs")), Some(ScriptLanguage::JavaScript)); assert_eq!(ScriptLanguage::from_path(Path::new("node.py")), Some(ScriptLanguage::Python)); assert_eq!(ScriptLanguage::from_path(Path::new("node.rs")), None); }
}
