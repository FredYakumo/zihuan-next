use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
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

fn message(value: impl Into<String>) -> EngineError {
    EngineError::Message(value.into())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum ScriptLanguage {
    JavaScript,
    Python,
}

impl ScriptLanguage {
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension().and_then(|value| value.to_str()) {
            Some("mjs") => Some(Self::JavaScript),
            Some("py") => Some(Self::Python),
            _ => None,
        }
    }

    fn runner_file(self) -> &'static str {
        match self {
            Self::JavaScript => "engine.mjs",
            Self::Python => "engine_runtime.py",
        }
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
pub enum PythonRuntimeKind {
    #[default]
    UvProject,
    #[serde(alias = "venv_python")]
    ProjectVenv,
    CustomExecutable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PythonRuntimeConfig {
    #[serde(default)]
    pub kind: PythonRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
}
impl Default for PythonRuntimeConfig {
    fn default() -> Self {
        Self {
            kind: PythonRuntimeKind::UvProject,
            executable_path: None,
        }
    }
}
impl From<PythonRuntimeKind> for PythonRuntimeConfig {
    fn from(kind: PythonRuntimeKind) -> Self {
        Self { kind, executable_path: None }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeRuntimeKind {
    #[default]
    ProjectNode,
    CustomExecutable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRuntimeConfig {
    #[serde(default)]
    pub kind: NodeRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
}
impl Default for NodeRuntimeConfig {
    fn default() -> Self {
        Self {
            kind: NodeRuntimeKind::ProjectNode,
            executable_path: None,
        }
    }
}
impl From<NodeRuntimeKind> for NodeRuntimeConfig {
    fn from(kind: NodeRuntimeKind) -> Self {
        Self { kind, executable_path: None }
    }
}

/// Environment pinned on every script runtime process.
///
/// Runners report manifests and results as JSON on stdout. A Python interpreter on Windows
/// defaults its stdio encoding to the console code page, so non-ASCII text comes back as
/// bytes that are not valid UTF-8, and the host parses those streams strictly. Pinning UTF-8
/// on the child keeps the two sides agreeing on encoding regardless of the host's locale.
const SCRIPT_RUNTIME_ENV: [(&str, &str); 2] = [("PYTHONUTF8", "1"), ("PYTHONIOENCODING", "utf-8")];

#[derive(Debug, Clone)]
pub struct RuntimeCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}
impl RuntimeCommand {
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command.envs(SCRIPT_RUNTIME_ENV);
        command
    }
    pub fn display(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn resolve_node_runtime(
    workspace: &Path,
    config: &NodeRuntimeConfig,
) -> Result<RuntimeCommand> {
    match config.kind {
        NodeRuntimeKind::ProjectNode => {
            let package = engine_dir(workspace).join("package.json");
            if !package.is_file() {
                return Err(message(format!("未检测到动态脚本运行时项目: {}", package.display())));
            }
            Ok(RuntimeCommand {
                program: PathBuf::from("node"),
                args: Vec::new(),
            })
        }
        NodeRuntimeKind::CustomExecutable => Ok(RuntimeCommand {
            program: custom_executable(
                workspace,
                config.executable_path.as_deref(),
                "动态脚本运行时的 Node.js 可执行文件",
            )?,
            args: Vec::new(),
        }),
    }
}

pub fn resolve_python_runtime(
    workspace: &Path,
    config: &PythonRuntimeConfig,
) -> Result<RuntimeCommand> {
    match config.kind {
        PythonRuntimeKind::UvProject => Ok(RuntimeCommand {
            program: PathBuf::from("uv"),
            args: vec!["run".to_string(), "python".to_string()],
        }),
        PythonRuntimeKind::ProjectVenv => {
            let executable = project_venv_python_path(workspace);
            if !executable.is_file() {
                return Err(message(format!(
                    "项目 Python 虚拟环境不存在: {}",
                    executable.display()
                )));
            }
            Ok(RuntimeCommand { program: executable, args: Vec::new() })
        }
        PythonRuntimeKind::CustomExecutable => Ok(RuntimeCommand {
            program: custom_executable(
                workspace,
                config.executable_path.as_deref(),
                "自定义 Python 解释器",
            )?,
            args: Vec::new(),
        }),
    }
}

pub fn project_venv_python_path(workspace: &Path) -> PathBuf {
    if cfg!(windows) {
        workspace.join(".venv/Scripts/python.exe")
    } else {
        workspace.join(".venv/bin/python")
    }
}
fn engine_dir(workspace: &Path) -> PathBuf {
    workspace.join("dynamic_script_engine")
}
fn custom_executable(workspace: &Path, raw: Option<&str>, label: &str) -> Result<PathBuf> {
    let raw = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| message(format!("{label}路径不能为空")))?;
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    };
    if !path.is_file() {
        return Err(message(format!("{label}不存在: {}", path.display())));
    }
    Ok(path)
}

/// Resolves the command that launches one language runner.
///
/// The Node command additionally carries the TypeScript type-stripping flags, because script
/// tools may be authored as `.ts` modules. `.mjs` and `.py` scripts are unaffected: the flags
/// are inert for them, and they are never added when the installed Node cannot accept them.
pub fn resolve_runner_command(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
) -> Result<RuntimeCommand> {
    match language {
        ScriptLanguage::JavaScript => {
            let mut command = resolve_node_runtime(workspace, node)?;
            command.args.extend(node_type_strip_args(&command.program));
            Ok(command)
        }
        ScriptLanguage::Python => resolve_python_runtime(workspace, python),
    }
}

/// Verifies that the resolved Node.js can import TypeScript modules directly.
///
/// Type stripping reached Node in 22.6; that is the floor a `.ts` script tool needs. Older
/// runtimes (and undetectable ones) are rejected here with a readable message instead of
/// failing later with Node's own extension error.
pub fn require_typescript_node(workspace: &Path, node: &NodeRuntimeConfig) -> Result<()> {
    let command = resolve_node_runtime(workspace, node)?;
    match node_version(&command.program) {
        Some((major, minor)) if major > 22 || (major == 22 && minor >= 6) => Ok(()),
        Some((major, minor)) => Err(message(format!(
            "当前 Node.js（{major}.{minor}）无法直接运行 TypeScript 脚本，请升级到 Node.js 22.6 以上，或改用 Python 脚本"
        ))),
        None => Err(message(format!(
            "无法检测动态脚本运行时的 Node.js 版本: {}",
            command.display()
        ))),
    }
}

/// Type-stripping flags for the Node command, empty when the runtime strips types by default.
fn node_type_strip_args(program: &Path) -> Vec<String> {
    match node_version(program) {
        // 22.18 and 23.6 turned type stripping on by default.
        Some((major, minor))
            if major > 23 || (major == 23 && minor >= 6) || (major == 22 && minor >= 18) =>
        {
            Vec::new()
        }
        // Between 22.6 and those releases the flag is required.
        Some((major, minor)) if major > 22 || (major == 22 && minor >= 6) => {
            vec!["--experimental-strip-types".to_string()]
        }
        _ => Vec::new(),
    }
}

/// The major and minor of an interpreter, as `node --version` reports them.
type NodeVersion = (u32, u32);

/// Reads the major/minor of the resolved Node.js, caching per executable.
///
/// The probe runs at most once per interpreter path: it costs a process spawn, and the answer
/// cannot change while the service is up.
fn node_version(program: &Path) -> Option<NodeVersion> {
    static CACHE: Lazy<Mutex<HashMap<PathBuf, Option<NodeVersion>>>> =
        Lazy::new(|| Mutex::new(HashMap::new()));
    if let Some(cached) = CACHE.lock().ok().and_then(|cache| cache.get(program).copied()) {
        return cached;
    }
    let version = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| parse_node_version(&String::from_utf8_lossy(&output.stdout)));
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(program.to_path_buf(), version);
    }
    version
}

fn parse_node_version(raw: &str) -> Option<NodeVersion> {
    let mut parts = raw.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

pub async fn check_node_runtime(
    workspace: &Path,
    config: &NodeRuntimeConfig,
) -> Result<(RuntimeCommand, String, String)> {
    check_runtime(resolve_node_runtime(workspace, config)?, workspace, "动态脚本运行时").await
}
pub async fn check_python_runtime(
    workspace: &Path,
    config: &PythonRuntimeConfig,
) -> Result<(RuntimeCommand, String, String)> {
    let command = resolve_python_runtime(workspace, config)?;
    if config.kind == PythonRuntimeKind::UvProject {
        let project = workspace.join("pyproject.toml");
        if !project.is_file() {
            return Err(message(format!("未检测到 pyproject.toml: {}", project.display())));
        }
        let output = timed_output(
            {
                let mut cmd = Command::new("uv");
                cmd.arg("--version");
                cmd
            },
            "uv",
        )
        .await?;
        if !output.status.success() {
            return Err(command_failure("uv 运行时检测失败", &output.stderr));
        }
        return Ok((
            command,
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            project.display().to_string(),
        ));
    }
    check_runtime(command, workspace, "Python 运行时").await
}
async fn check_runtime(
    command: RuntimeCommand,
    workspace: &Path,
    label: &str,
) -> Result<(RuntimeCommand, String, String)> {
    let mut process = command.to_command();
    process.arg("--version").current_dir(workspace);
    let output = timed_output(process, label).await?;
    if !output.status.success() {
        return Err(command_failure(&format!("{label}检测失败"), &output.stderr));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if version.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version
    };
    Ok((command.clone(), version, command.program.display().to_string()))
}
async fn timed_output(command: Command, label: &str) -> Result<std::process::Output> {
    let program = command.get_program().to_owned();
    let args: Vec<_> = command.get_args().map(|arg| arg.to_owned()).collect();
    let directory = command.get_current_dir().map(PathBuf::from);
    let mut command = tokio::process::Command::new(program);
    command.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    command.envs(SCRIPT_RUNTIME_ENV);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let mut child =
        command.spawn().map_err(|error| message(format!("无法启动{label}: {error}")))?;
    let status = match timeout(Duration::from_secs(10), child.wait()).await {
        Ok(result) => result.map_err(|error| message(format!("无法等待{label}: {error}")))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(message(format!("{label}检测超时（10 秒）")));
        }
    };
    let mut stdout = Vec::new();
    if let Some(mut stream) = child.stdout.take() {
        stream
            .read_to_end(&mut stdout)
            .await
            .map_err(|error| message(format!("无法读取{label}输出: {error}")))?;
    }
    let mut stderr = Vec::new();
    if let Some(mut stream) = child.stderr.take() {
        stream
            .read_to_end(&mut stderr)
            .await
            .map_err(|error| message(format!("无法读取{label}错误输出: {error}")))?;
    }
    Ok(std::process::Output { status, stdout, stderr })
}
fn command_failure(prefix: &str, stderr: &[u8]) -> EngineError {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    message(format!(
        "{prefix}: {}",
        if stderr.is_empty() {
            "暂未检测到"
        } else {
            &stderr
        }
    ))
}

pub fn discover_languages(workspace: &Path) -> Result<BTreeSet<ScriptLanguage>> {
    discover_languages_in(&workspace.join("dag_nodes"))
}

fn discover_languages_in(directory: &Path) -> Result<BTreeSet<ScriptLanguage>> {
    fn visit(directory: &Path, languages: &mut BTreeSet<ScriptLanguage>) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit(&path, languages)?;
            } else if let Some(language) = ScriptLanguage::from_path(&path) {
                languages.insert(language);
            }
        }
        Ok(())
    }
    let mut languages = BTreeSet::new();
    if !directory.is_dir() {
        return Ok(languages);
    }
    visit(directory, &mut languages)?;
    Ok(languages)
}

/// One scheduler job manifest reported by a runner's `--jobs-catalog` mode. `script` is
/// relative to the job directory that was scanned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCatalogEntry {
    pub task_name: String,
    pub script: String,
    #[serde(default = "default_job_entry")]
    pub entry: String,
    #[serde(default)]
    pub description: String,
}

fn default_job_entry() -> String {
    "run_job".to_string()
}

/// Aggregated scheduler job manifest catalog across script runtimes. Runner or script
/// failures become diagnostics so one broken job cannot hide the others.
#[derive(Debug, Clone, Default)]
pub struct JobCatalog {
    pub jobs: Vec<JobCatalogEntry>,
    pub diagnostics: Vec<ScriptDiagnostic>,
}

/// Recursively collects the job scripts under `directory`, pairing each script's POSIX
/// display path (relative to `directory`) with the language its extension implies.
/// Other extensions are ignored and the result is sorted into a deterministic order.
fn job_scripts_in(directory: &Path) -> Result<Vec<(String, ScriptLanguage)>> {
    fn visit(
        directory: &Path,
        base: &Path,
        scripts: &mut Vec<(String, ScriptLanguage)>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(&path, base, scripts)?;
            } else if let Some(language) = ScriptLanguage::from_path(&path) {
                let display = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                scripts.push((display, language));
            }
        }
        Ok(())
    }
    let mut scripts = Vec::new();
    if !directory.is_dir() {
        return Ok(scripts);
    }
    visit(directory, directory, &mut scripts)?;
    scripts.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(scripts)
}

/// Loads scheduler job manifests declared inside the scripts of `job_directory`.
///
/// Traversal lives in Rust: the scripts are discovered here and handed to the language
/// runner as an explicit path list, so a runner only imports the files it is given and
/// reports the metadata they export (`job_manifest` export for `.mjs`, `JOB_MANIFEST` dict
/// for `.py`).
pub fn load_job_catalog(
    workspace: &Path,
    job_directory: &Path,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
) -> Result<JobCatalog> {
    let mut catalog = JobCatalog::default();
    let mut paths_by_language: BTreeMap<ScriptLanguage, Vec<String>> = BTreeMap::new();
    for (display, language) in job_scripts_in(job_directory)? {
        paths_by_language.entry(language).or_default().push(display);
    }
    for (language, paths) in paths_by_language {
        let command = resolve_runner_command(workspace, language, node, python);
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                catalog
                    .diagnostics
                    .push(ScriptDiagnostic { language, message: error.to_string() });
                continue;
            }
        };
        match run_once(
            workspace,
            language,
            &command,
            "--jobs-catalog",
            Some(&json!({ "paths": paths })),
        ) {
            Ok(Value::Object(mut response)) => {
                let diagnostics: Vec<ScriptDiagnostic> = response
                    .remove("diagnostics")
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
                catalog.diagnostics.extend(diagnostics);
                let jobs = response
                    .remove("jobs")
                    .and_then(|value| value.as_array().cloned())
                    .unwrap_or_default();
                for job in jobs {
                    // A malformed manifest is that script's own problem: report it and keep the
                    // other jobs, so one broken script cannot abort the whole catalog.
                    let script = job
                        .get("script")
                        .and_then(Value::as_str)
                        .unwrap_or("<unknown>")
                        .to_string();
                    match serde_json::from_value::<JobCatalogEntry>(job) {
                        Ok(entry) => catalog.jobs.push(entry),
                        Err(error) => catalog.diagnostics.push(ScriptDiagnostic {
                            language,
                            message: format!("{script} 的 job 清单无效: {error}"),
                        }),
                    }
                }
            }
            Ok(_) => return Err(message(format!("{language:?} job 目录响应无效"))),
            Err(error) => catalog
                .diagnostics
                .push(ScriptDiagnostic { language, message: error.to_string() }),
        }
    }
    Ok(catalog)
}

pub fn load_script_catalog(
    workspace: &Path,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
) -> Result<ScriptCatalog> {
    let mut catalog = ScriptCatalog {
        nodes: Vec::new(),
        diagnostics: Vec::new(),
    };
    for language in discover_languages(workspace)? {
        let command = resolve_runner_command(workspace, language, node, python);
        let command = match command {
            Ok(command) => command,
            Err(error) => {
                catalog
                    .diagnostics
                    .push(ScriptDiagnostic { language, message: error.to_string() });
                continue;
            }
        };
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
                for mut definition in nodes {
                    definition
                        .as_object_mut()
                        .ok_or_else(|| message(format!("{language:?} 节点定义必须是对象")))?
                        .insert("language".to_string(), serde_json::to_value(language)?);
                    catalog.nodes.push(definition);
                }
            }
            Ok(_) => return Err(message(format!("{language:?} 节点目录响应无效"))),
            Err(error) => catalog
                .diagnostics
                .push(ScriptDiagnostic { language, message: error.to_string() }),
        }
    }
    let mut ids = BTreeSet::new();
    for definition in &catalog.nodes {
        let type_id = definition
            .get("type_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| message("动态节点缺少 type_id"))?;
        if !ids.insert(type_id.to_string()) {
            return Err(message(format!("动态节点 type_id 重复: {type_id}")));
        }
    }
    Ok(catalog)
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}
impl Worker {
    fn start(workspace: &Path, language: ScriptLanguage, command: &RuntimeCommand) -> Result<Self> {
        let mut child = command
            .to_command()
            .arg(engine_dir(workspace).join(language.runner_file()))
            .arg("--serve")
            .current_dir(engine_dir(workspace))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        Ok(Self {
            stdin: child.stdin.take().ok_or_else(|| message("无法打开动态脚本运行时 stdin"))?,
            stdout: BufReader::new(
                child.stdout.take().ok_or_else(|| message("无法打开动态脚本运行时 stdout"))?,
            ),
            child,
        })
    }
    fn running(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_none())
    }
    fn child_id(&self) -> u32 {
        self.child.id()
    }
    fn request(&mut self, request: &Value, host: &mut HostHandler<'_>) -> Result<Value> {
        serde_json::to_writer(&mut self.stdin, request)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        loop {
            let mut line = String::new();
            if self.stdout.read_line(&mut line)? == 0 {
                return Err(message("动态脚本运行时在响应前退出"));
            }
            let response: Value = serde_json::from_str(&line)
                .map_err(|error| message(format!("动态脚本运行时响应无效: {error}")))?;
            if response.get("kind").and_then(Value::as_str) != Some("host_request") {
                return Ok(response);
            }
            let id = response.get("id").cloned().unwrap_or(Value::Null);
            let reply = match host(
                response.get("method").and_then(Value::as_str).unwrap_or_default(),
                response.get("params").unwrap_or(&Value::Null),
            ) {
                Ok(result) => json!({"kind":"host_response","id":id,"result":result}),
                Err(error) => json!({"kind":"host_response","id":id,"error":error}),
            };
            serde_json::to_writer(&mut self.stdin, &reply)?;
            self.stdin.write_all(b"\n")?;
            self.stdin.flush()?;
        }
    }
}
static WORKERS: Lazy<Mutex<HashMap<ScriptLanguage, Worker>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn start_script_runtime(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
) -> Result<()> {
    let command = resolve_runner_command(workspace, language, node, python)?;
    let mut workers = WORKERS.lock().map_err(|_| message("动态脚本运行时互斥锁不可用"))?;
    if workers
        .get_mut(&language)
        .map(|worker| worker.running())
        .transpose()?
        .unwrap_or(false)
    {
        return Ok(());
    }
    workers.insert(language, Worker::start(workspace, language, &command)?);
    Ok(())
}
/// Sends one request to the worker for `language`, starting it on demand.
///
/// `limit` bounds the whole exchange the way [`execute_script_tool_with_sdk`] needs: a script
/// that overruns is killed (see [`TimeoutWatchdog`]) and its worker dropped, so the failure
/// surfaces as an error and the next request starts a fresh runner. A failed protocol exchange
/// always drops the worker, because the runner has no way to resynchronize its line stream.
pub fn request_script_runtime(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    request: &Value,
    host: &mut HostHandler<'_>,
    limit: Option<Duration>,
) -> Result<Value> {
    start_script_runtime(workspace, language, node, python)?;
    let mut workers = WORKERS.lock().map_err(|_| message("动态脚本运行时互斥锁不可用"))?;
    let worker = workers.get_mut(&language).expect("worker initialized");
    let watchdog = limit.map(|limit| TimeoutWatchdog::start(worker.child_id(), limit));
    let result = worker.request(request, host);
    if let Some(watchdog) = watchdog {
        if watchdog.disarm() {
            workers.remove(&language);
            return Err(message(format!(
                "脚本运行超时（{} 秒）已终止",
                limit.unwrap_or_default().as_secs()
            )));
        }
    }
    if result.is_err() {
        workers.remove(&language);
    }
    result
}
pub fn resolve_script_ports(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    request: &Value,
) -> Result<Value> {
    let command = resolve_runner_command(workspace, language, node, python)?;
    run_once(workspace, language, &command, "--ports", Some(request))
}
fn run_once(
    workspace: &Path,
    language: ScriptLanguage,
    command: &RuntimeCommand,
    argument: &str,
    request: Option<&Value>,
) -> Result<Value> {
    run_once_timed(workspace, language, command, argument, request, None)
}
fn run_once_timed(
    workspace: &Path,
    language: ScriptLanguage,
    command: &RuntimeCommand,
    argument: &str,
    request: Option<&Value>,
    limit: Option<Duration>,
) -> Result<Value> {
    let mut command = command.to_command();
    command
        .arg(engine_dir(workspace).join(language.runner_file()))
        .arg(argument)
        .current_dir(engine_dir(workspace))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if request.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    let watchdog = limit.map(|limit| TimeoutWatchdog::start(child.id(), limit));
    if let Some(request) = request {
        serde_json::to_writer(
            child.stdin.as_mut().ok_or_else(|| message("无法打开动态脚本运行时 stdin"))?,
            request,
        )?;
    }
    let output = child.wait_with_output()?;
    let timed_out = watchdog.map(|watchdog| watchdog.disarm()).unwrap_or(false);
    if timed_out {
        return Err(message(format!(
            "脚本运行超时（{} 秒）已终止",
            limit.unwrap_or_default().as_secs()
        )));
    }
    if !output.status.success() {
        return Err(message(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| message(format!("动态脚本运行时响应不是合法 JSON: {error}")))
}

/// Terminates a runner that overran its deadline.
///
/// The runner protocol is one line in, one line out, with no cancellation frame, so an
/// overrunning script is killed and its worker discarded; the next request starts a fresh one.
struct TimeoutWatchdog {
    done: Arc<AtomicBool>,
}

impl TimeoutWatchdog {
    fn start(pid: u32, limit: Duration) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&done);
        thread::spawn(move || {
            let deadline = std::time::Instant::now() + limit;
            while std::time::Instant::now() < deadline {
                if flag.load(Ordering::Acquire) {
                    return;
                }
                thread::sleep(Duration::from_millis(50).min(limit));
            }
            if !flag.load(Ordering::Acquire) {
                terminate_process(pid);
            }
        });
        Self { done }
    }

    /// Stops the countdown, reporting whether the watched process was already terminated.
    fn disarm(self) -> bool {
        self.done.swap(true, Ordering::AcqRel)
    }
}

#[cfg(windows)]
fn terminate_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(windows))]
fn terminate_process(pid: u32) {
    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Runs a tool-style script request: the script file is loaded on demand, its exported
/// `entry` function receives the request object (plus `script_path` and `entry`), and the
/// returned value is expected to follow the `{ ok: bool, result?/error? }` contract.
///
/// Works for both [`ScriptLanguage::Python`] and [`ScriptLanguage::JavaScript`]; the JS
/// runtime (`engine.mjs --serve`) dispatches the same `tool_execute` request kind.
#[allow(clippy::too_many_arguments)]
pub fn execute_script_tool(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    script_path: &Path,
    entry: &str,
    request: &Value,
    host: &mut HostHandler<'_>,
) -> Result<Value> {
    execute_tool_request(
        workspace,
        language,
        node,
        python,
        "tool_execute",
        script_path,
        entry,
        None,
        request,
        host,
    )
}

/// Runs an agent script tool: same contract as [`execute_script_tool`], but the entry receives
/// the ZiHuan SDK facade as its second argument and the call is bounded by `timeout_secs`.
///
/// The entry signature is `entry(request, zihuan)`; that is the difference from a scheduler job
/// script, which builds its own job facade from the request alone.
#[allow(clippy::too_many_arguments)]
pub fn execute_script_tool_with_sdk(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    script_path: &Path,
    entry: &str,
    timeout_secs: u64,
    request: &Value,
    host: &mut HostHandler<'_>,
) -> Result<Value> {
    let limit = if timeout_secs == 0 {
        None
    } else {
        Some(Duration::from_secs(timeout_secs))
    };
    execute_tool_request(
        workspace,
        language,
        node,
        python,
        "script_tool_execute",
        script_path,
        entry,
        limit,
        request,
        host,
    )
}

/// Reads the parameters and outputs a script declares, without running its tool entry.
///
/// Script tools keep their LLM-facing signature next to the code that serves it: a Python script
/// exports `TOOL_MANIFEST`, a JavaScript or TypeScript one exports `tool_manifest`. Scripts that
/// declare nothing come back empty, and the caller falls back to hand-written parameters.
pub fn load_tool_manifest(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    script_path: &Path,
) -> Result<Value> {
    let script_path = resolve_script_path(workspace, script_path)?;
    let command = resolve_runner_command(workspace, language, node, python)?;
    let response = run_once_timed(
        workspace,
        language,
        &command,
        "--tool-manifest",
        Some(&json!({ "script_path": script_path.display().to_string() })),
        Some(Duration::from_secs(60)),
    )?;
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(message(error.to_string()));
    }
    Ok(response)
}

#[allow(clippy::too_many_arguments)]
fn execute_tool_request(
    workspace: &Path,
    language: ScriptLanguage,
    node: &NodeRuntimeConfig,
    python: &PythonRuntimeConfig,
    request_kind: &str,
    script_path: &Path,
    entry: &str,
    limit: Option<Duration>,
    request: &Value,
    host: &mut HostHandler<'_>,
) -> Result<Value> {
    let script_path = resolve_script_path(workspace, script_path)?;
    let mut request = request.clone();
    let object = request.as_object_mut().ok_or_else(|| message("脚本工具请求必须是对象"))?;
    object.insert("script_path".to_string(), Value::String(script_path.display().to_string()));
    object.insert("entry".to_string(), Value::String(entry.to_string()));
    let response = request_script_runtime(
        workspace,
        language,
        node,
        python,
        &json!({"kind": request_kind, "request": request}),
        host,
        limit,
    )?;
    if let Some(result) = response.get("response") {
        return Ok(result.clone());
    }
    if let Some(error) = response.get("error").and_then(Value::as_str) {
        return Err(message(error.to_string()));
    }
    Err(message("脚本工具响应缺少 response"))
}

fn resolve_script_path(workspace: &Path, script_path: &Path) -> Result<PathBuf> {
    let script_path = if script_path.is_absolute() {
        script_path.to_path_buf()
    } else {
        workspace.join(script_path)
    };
    if !script_path.is_file() {
        return Err(message(format!("脚本文件不存在: {}", script_path.display())));
    }
    Ok(script_path)
}
