//! Scheduler job scripts: every script under `scheduled_jobs/` declares its own manifest
//! inline (`job_manifest` export for `.mjs`, `JOB_MANIFEST` dict for `.py`) and the script
//! runners report those manifests through the `--jobs-catalog` catalog mode.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};

use dynamic_script_engine::{JobCatalog, NodeRuntimeConfig, PythonRuntimeConfig, ScriptLanguage};

use super::capabilities;
use super::{JobRunContext, SchedulerJob};
use crate::config::ConfigCenter;
use crate::error::{Error, Result};

/// Task name of the built-in Dream consolidation job (`scheduled_jobs/dream_job.py`).
pub const DREAM_TASK_NAME: &str = "Dream";

/// Directory holding scheduler job scripts, relative to the workspace.
pub fn scheduled_jobs_dir() -> PathBuf {
    PathBuf::from("scheduled_jobs")
}

/// A scheduler job shipped with the application instead of being provided by an operator.
/// `source` is embedded so a fresh install can seed its `scheduled_jobs` directory, and
/// `task_name` is the registry identity the seeding step and the runtime agree on.
pub struct BuiltinScheduler {
    /// `task_name` the job registers under.
    pub task_name: &'static str,
    /// File name the script is seeded as inside `scheduled_jobs/`.
    pub script_file: &'static str,
    /// Embedded script source, written only when the file is absent.
    pub source: &'static str,
}

/// Every built-in scheduler, in seeding order. Listing a job here is what makes it part of
/// the application; the `scheduled_jobs/` directory still decides which body actually runs.
pub const BUILTIN_SCHEDULERS: &[BuiltinScheduler] = &[BuiltinScheduler {
    task_name: DREAM_TASK_NAME,
    script_file: "dream_job.py",
    source: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../scheduled_jobs/dream_job.py")),
}];

/// The built-in scheduler list, for callers reasoning about shipped jobs.
pub fn builtin_schedulers() -> &'static [BuiltinScheduler] {
    BUILTIN_SCHEDULERS
}

/// Whether `task_name` belongs to a scheduler shipped with the application.
pub fn is_builtin_scheduler(task_name: &str) -> bool {
    BUILTIN_SCHEDULERS.iter().any(|scheduler| scheduler.task_name == task_name)
}

/// Script file names owned by the built-in schedulers.
fn builtin_script_files() -> Vec<&'static str> {
    builtin_schedulers().iter().map(|scheduler| scheduler.script_file).collect()
}

/// The identity a job script declares for itself while the job directory is scanned.
///
/// The declared name is the key every other part of the system refers to the job by, so two
/// scripts must not declare the same one.
#[derive(Debug, Clone)]
pub struct JobManifest {
    pub task_name: String,
    pub script: String,
    pub entry: String,
    pub description: String,
}

impl JobManifest {
    fn language(&self) -> Result<ScriptLanguage> {
        let path = scheduled_jobs_dir().join(&self.script);
        if !path.is_file() {
            return Err(Error::ValidationError(format!("job 脚本不存在: {}", path.display())));
        }
        ScriptLanguage::from_path(&path).ok_or_else(|| {
            Error::ValidationError(format!("job 脚本必须是 .mjs 或 .py 文件: {}", path.display()))
        })
    }
}

fn manifest_from_entry(entry: dynamic_script_engine::JobCatalogEntry) -> Result<JobManifest> {
    if entry.task_name.trim().is_empty() {
        return Err(Error::ValidationError("job 清单 task_name 不能为空".to_string()));
    }
    Ok(JobManifest {
        task_name: entry.task_name,
        script: entry.script,
        entry: entry.entry,
        description: entry.description,
    })
}

/// Creates the built-in job scripts that are absent at application startup.
/// Existing files are intentionally left untouched.
pub fn ensure_default_jobs() -> Result<()> {
    let directory = scheduled_jobs_dir();
    fs::create_dir_all(&directory).map_err(|error| {
        Error::ValidationError(format!("failed to create scheduled jobs directory: {error}"))
    })?;
    for scheduler in builtin_schedulers() {
        let path = directory.join(scheduler.script_file);
        if path.exists() {
            continue;
        }
        fs::write(&path, scheduler.source).map_err(|error| {
            Error::ValidationError(format!(
                "failed to write built-in job script '{}': {error}",
                scheduler.script_file
            ))
        })?;
    }
    Ok(())
}

/// Reads the manifests the script runners report for `scheduled_jobs/`.
fn load_catalog(
    workspace: &Path,
    node_runtime: &NodeRuntimeConfig,
    python_runtime: &PythonRuntimeConfig,
) -> Result<JobCatalog> {
    dynamic_script_engine::load_job_catalog(
        workspace,
        &scheduled_jobs_dir(),
        node_runtime,
        python_runtime,
    )
    .map_err(|error| crate::string_error!("加载 scheduled_jobs 目录失败: {error}"))
}

/// Rewrites the built-in job scripts whose `task_name` is absent from `catalog`, so a job
/// shipped with the application cannot stay unregistered because its file was deleted,
/// emptied, or corrupted. A file that differs from the embedded source is kept as a `.bak`
/// sibling first, so an operator edit is never silently lost. Returns the re-read catalog
/// when a file was rewritten, since `catalog` no longer describes the directory.
fn restore_missing_builtins(
    workspace: &Path,
    catalog: &JobCatalog,
    node_runtime: &NodeRuntimeConfig,
    python_runtime: &PythonRuntimeConfig,
) -> Result<Option<JobCatalog>> {
    let mut repaired = false;
    for scheduler in builtin_schedulers() {
        let present = catalog.jobs.iter().any(|entry| entry.task_name == scheduler.task_name);
        if present {
            continue;
        }
        let path = scheduled_jobs_dir().join(scheduler.script_file);
        let current = fs::read(&path).ok();
        // The file already matches the shipped script, so rewriting it cannot help: whatever
        // keeps the manifest from loading lies outside the file (runtime, encoding, permissions).
        if current.as_deref() == Some(scheduler.source.as_bytes()) {
            log::warn!(
                "[Scheduler] built-in job '{}'（{}）未注册，但脚本内容与内置一致，请检查脚本运行时",
                scheduler.task_name,
                scheduler.script_file
            );
            continue;
        }
        if let Some(previous) = &current {
            let backup = path.with_file_name(format!("{}.bak", scheduler.script_file));
            match fs::write(&backup, previous) {
                Ok(()) => log::warn!(
                    "[Scheduler] 原 {} 已备份为 {}",
                    scheduler.script_file,
                    backup.display()
                ),
                Err(error) => log::warn!(
                    "[Scheduler] 无法备份 {} 为 {}: {error}",
                    path.display(),
                    backup.display()
                ),
            }
        }
        fs::write(&path, scheduler.source).map_err(|error| {
            Error::ValidationError(format!(
                "failed to restore built-in job script '{}': {error}",
                scheduler.script_file
            ))
        })?;
        log::warn!(
            "[Scheduler] built-in job '{}'（{}）未注册，已从内置脚本恢复",
            scheduler.task_name,
            scheduler.script_file
        );
        repaired = true;
    }
    if !repaired {
        return Ok(None);
    }
    // The restored files are on disk either way, so a failed re-read is not fatal: the next
    // start re-scans the directory and registers them.
    match load_catalog(workspace, node_runtime, python_runtime) {
        Ok(reloaded) => Ok(Some(reloaded)),
        Err(error) => {
            log::warn!("[Scheduler] 恢复内置 job 后重新读取失败: {error}");
            Ok(None)
        }
    }
}

/// Registers a job body per manifest in `catalog`. Per-job failures are logged and skipped so
/// one broken job cannot hide the others. The caller owns deciding which catalog to hand in and
/// whether the registry should be cleared first.
fn register_catalog(
    workspace: &Path,
    catalog: JobCatalog,
    node_runtime: &NodeRuntimeConfig,
    python_runtime: &PythonRuntimeConfig,
) -> usize {
    for diagnostic in &catalog.diagnostics {
        log::warn!("[Scheduler] job 目录诊断: {}", diagnostic.message);
    }
    // `task_name` -> the script that already claimed it, so a duplicate can name both sides.
    let mut registered_scripts: HashMap<String, String> = HashMap::new();
    // Built-ins are the lowest-precedence claimant of their `task_name`: registering them last
    // lets a same-named script in `scheduled_jobs/` override the shipped job.
    let builtin_files = builtin_script_files();
    let mut entries = catalog.jobs;
    entries.sort_by_key(|entry| builtin_files.contains(&entry.script.as_str()));
    let mut count = 0;
    for entry in entries {
        let manifest = match manifest_from_entry(entry) {
            Ok(manifest) => manifest,
            Err(error) => {
                log::warn!("[Scheduler] {}", error);
                continue;
            }
        };
        if let Some(owner) = registered_scripts.get(&manifest.task_name) {
            // A built-in yielding to an operator script is the documented override, not a
            // conflict, so it is reported at info level rather than as an error.
            if builtin_files.contains(&manifest.script.as_str()) {
                log::info!(
                    "[Scheduler] 内置 job '{}'（{}）已被 {} 覆盖",
                    manifest.task_name,
                    manifest.script,
                    owner
                );
            } else {
                log::error!(
                    "[Scheduler] job task_name 重复 {}（task_name '{}' 已由 {} 注册）",
                    manifest.script,
                    manifest.task_name,
                    owner
                );
            }
            continue;
        }
        let language = match manifest.language() {
            Ok(language) => language,
            Err(error) => {
                log::warn!("[Scheduler] {}", error);
                continue;
            }
        };
        if let Err(error) = dynamic_script_engine::start_script_runtime(
            workspace,
            language,
            node_runtime,
            python_runtime,
        ) {
            log::warn!("[Scheduler] failed to start script runtime for {language:?} jobs: {error}");
        }
        let task_name = manifest.task_name.clone();
        let script = manifest.script.clone();
        super::register_job(manifest.clone(), Arc::new(ScriptJob { manifest, language }));
        registered_scripts.insert(task_name, script);
        count += 1;
    }
    // A built-in job that failed to register is a defect rather than an operator choice, so
    // name it explicitly instead of letting it disappear into the catalog diagnostics.
    for scheduler in builtin_schedulers() {
        if !registered_scripts.contains_key(scheduler.task_name) {
            log::warn!(
                "[Scheduler] built-in job '{}'（{}）未注册：脚本缺失或清单无效",
                scheduler.task_name,
                scheduler.script_file
            );
        }
    }
    count
}

/// Seeds default job files, collects the manifests the script runners report from
/// `scheduled_jobs/`, and registers a script job body per `task_name`.
pub fn init_script_jobs() -> Result<usize> {
    ensure_default_jobs()?;
    let workspace = std::env::current_dir()
        .map_err(|error| Error::ValidationError(format!("failed to resolve workspace: {error}")))?;
    let (node_runtime, python_runtime) = runtime_configs()?;
    let mut catalog = load_catalog(&workspace, &node_runtime, &python_runtime)?;
    if let Some(reloaded) =
        restore_missing_builtins(&workspace, &catalog, &node_runtime, &python_runtime)?
    {
        catalog = reloaded;
    }
    Ok(register_catalog(&workspace, catalog, &node_runtime, &python_runtime))
}

/// Re-reads `scheduled_jobs/` and rebuilds the job registry, so an edited script or manifest
/// takes effect without restarting the service. Unlike [`init_script_jobs`] this only re-scans:
/// seeding and built-in restoration belong to startup, and re-running them here would undo a
/// deliberate deletion of a built-in script.
pub fn reload_script_jobs() -> Result<usize> {
    let workspace = std::env::current_dir()
        .map_err(|error| Error::ValidationError(format!("failed to resolve workspace: {error}")))?;
    let (node_runtime, python_runtime) = runtime_configs()?;
    let catalog = load_catalog(&workspace, &node_runtime, &python_runtime)?;
    // Clear first: `register_job` reports a duplicate `task_name` as an error, so a re-scan
    // without this would log one error per still-registered job.
    super::clear_jobs();
    Ok(register_catalog(&workspace, catalog, &node_runtime, &python_runtime))
}

fn runtime_configs() -> Result<(NodeRuntimeConfig, PythonRuntimeConfig)> {
    let root = ConfigCenter::shared().load_root()?;
    Ok((root.node_runtime, root.python_runtime))
}

/// Largest job script the management API will read or accept, matching the node script limit.
const MAX_JOB_SCRIPT_BYTES: u64 = 512 * 1024;

/// A job script that was written and then successfully registered.
#[derive(Debug, Serialize)]
pub struct SavedJobScript {
    pub script: String,
    pub task_name: String,
}

/// Whether `script` names a script the application ships, which operators may edit but not
/// delete: startup would seed it again anyway.
pub fn is_builtin_script(script: &str) -> bool {
    let script = script.replace('\\', "/");
    builtin_schedulers().iter().any(|scheduler| scheduler.script_file == script)
}

/// Resolves an operator-supplied script name to a path inside `scheduled_jobs/`, rejecting
/// anything that could escape the directory (absolute paths, `..`, or another extension).
fn resolve_job_script(script: &str) -> Result<PathBuf> {
    let relative = script.trim().replace('\\', "/");
    if relative.is_empty() {
        return Err(Error::ValidationError("脚本路径不能为空".to_string()));
    }
    let path = Path::new(&relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
                    | std::path::Component::ParentDir
                    | std::path::Component::CurDir
            )
        })
    {
        return Err(Error::ValidationError(format!(
            "脚本路径必须是 scheduled_jobs 下的相对路径: {script}"
        )));
    }
    if ScriptLanguage::from_path(path).is_none() {
        return Err(Error::ValidationError(format!("job 脚本必须是 .mjs 或 .py 文件: {script}")));
    }
    let directory = scheduled_jobs_dir();
    let resolved = directory.join(path);
    // Containment is checked on canonical paths so a symlink cannot point outside the directory.
    // For a script that does not exist yet, the parent directory is what must resolve inside.
    let canonical_dir = directory
        .canonicalize()
        .map_err(|error| crate::string_error!("scheduled_jobs 目录不可用: {error}"))?;
    let canonical = resolved
        .canonicalize()
        .or_else(|_| {
            resolved
                .parent()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?
                .canonicalize()
                .map(|parent| parent.join(path.file_name().unwrap_or_default()))
        })
        .map_err(|error| crate::string_error!("脚本路径无效: {error}"))?;
    if !canonical.starts_with(&canonical_dir) {
        return Err(Error::ValidationError(format!("脚本路径超出 scheduled_jobs 范围: {script}")));
    }
    Ok(resolved)
}

/// Reads one job script for editing.
pub fn read_job_script(script: &str) -> Result<String> {
    let path = resolve_job_script(script)?;
    if !path.is_file() {
        return Err(Error::ValidationError(format!("job 脚本不存在: {}", path.display())));
    }
    let size = fs::metadata(&path)
        .map_err(|error| crate::string_error!("读取 job 脚本失败: {error}"))?
        .len();
    if size > MAX_JOB_SCRIPT_BYTES {
        return Err(Error::ValidationError(format!(
            "job 脚本超过 {} KiB，无法在界面编辑",
            MAX_JOB_SCRIPT_BYTES / 1024
        )));
    }
    fs::read_to_string(&path).map_err(|error| crate::string_error!("读取 job 脚本失败: {error}"))
}

/// Writes a job script and registers it, rejecting content whose manifest cannot be read.
///
/// The write happens before validation so the manifest can be read back through the real
/// runners. When validation fails the previous file is restored, which keeps a job from
/// dropping out of the catalog — a job that fails to register disappears from the very list
/// the editor is reached from, leaving no way to repair it in the UI.
pub fn save_job_script(script: &str, content: &str) -> Result<SavedJobScript> {
    let path = resolve_job_script(script)?;
    if content.len() as u64 > MAX_JOB_SCRIPT_BYTES {
        return Err(Error::ValidationError(format!(
            "job 脚本超过 {} KiB",
            MAX_JOB_SCRIPT_BYTES / 1024
        )));
    }
    let script = script.trim().replace('\\', "/");
    let previous = fs::read(&path).ok();
    fs::write(&path, content)
        .map_err(|error| crate::string_error!("写入 job 脚本失败: {error}"))?;
    match register_after_write(&script) {
        Ok(task_name) => Ok(SavedJobScript { script, task_name }),
        Err(error) => {
            restore_script_file(&path, previous.as_deref());
            Err(error)
        }
    }
}

/// Re-scans the directory after a write, requiring the edited script to yield a usable manifest.
fn register_after_write(script: &str) -> Result<String> {
    let workspace = std::env::current_dir()
        .map_err(|error| Error::ValidationError(format!("failed to resolve workspace: {error}")))?;
    let (node_runtime, python_runtime) = runtime_configs()?;
    let catalog = load_catalog(&workspace, &node_runtime, &python_runtime)?;
    let entry = catalog
        .jobs
        .iter()
        .find(|entry| entry.script == script)
        .ok_or_else(|| manifest_rejection(script, &catalog))?;
    if entry.task_name.trim().is_empty() {
        return Err(Error::ValidationError(format!("{script} 的 job 清单 task_name 不能为空")));
    }
    // A `task_name` already owned by a different operator script is a real conflict. Claiming
    // a built-in's `task_name` stays allowed: overriding a shipped job is the documented rule.
    if let Some(owner) = catalog
        .jobs
        .iter()
        .find(|other| other.script != *script && other.task_name == entry.task_name)
    {
        if !builtin_script_files().contains(&owner.script.as_str()) {
            return Err(Error::ValidationError(format!(
                "task_name '{}' 已被 {} 使用",
                entry.task_name, owner.script
            )));
        }
    }
    let task_name = entry.task_name.clone();
    super::clear_jobs();
    register_catalog(&workspace, catalog, &node_runtime, &python_runtime);
    Ok(task_name)
}

/// Builds the error shown when the edited script produced no manifest, quoting the runner's own
/// diagnostic (a Python `SyntaxError`, a missing `JOB_MANIFEST`, ...) when there is one.
fn manifest_rejection(script: &str, catalog: &JobCatalog) -> Error {
    let detail = catalog
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains(script))
        .map(|diagnostic| diagnostic.message.clone());
    let message = match detail {
        Some(detail) => format!("{script} 未提供有效的 job 清单：{detail}"),
        // The runner reported nothing about this file, so name the declaration the file needs.
        None => format!("{script} 未提供 job 清单（需要 JOB_MANIFEST 或 job_manifest）"),
    };
    Error::ValidationError(message)
}

/// Restores the file a rejected save overwrote, or removes it when the save created it.
fn restore_script_file(path: &Path, previous: Option<&[u8]>) {
    let outcome = match previous {
        Some(previous) => fs::write(path, previous),
        None => fs::remove_file(path),
    };
    if let Err(error) = outcome {
        log::warn!("[Scheduler] 回滚 job 脚本 {} 失败: {error}", path.display());
    }
}

/// Deletes an operator-provided job script and re-registers the remaining ones. Built-in scripts
/// are refused because startup would seed them again.
pub fn delete_job_script(script: &str) -> Result<()> {
    let path = resolve_job_script(script)?;
    if is_builtin_script(script) {
        return Err(Error::ValidationError(format!(
            "内置 job 脚本不能删除（由应用提供）: {}",
            script.trim()
        )));
    }
    if !path.is_file() {
        return Err(Error::ValidationError(format!("job 脚本不存在: {}", path.display())));
    }
    fs::remove_file(&path).map_err(|error| crate::string_error!("删除 job 脚本失败: {error}"))?;
    reload_script_jobs()?;
    Ok(())
}

/// A job body whose behavior lives in a script instead of in Rust. Each run crosses into the
/// script runtime of the manifest's language and reports back a summary.
struct ScriptJob {
    manifest: JobManifest,
    language: ScriptLanguage,
}

impl SchedulerJob for ScriptJob {
    fn run(&self, context: &JobRunContext) -> Result<Option<String>> {
        let workspace = std::env::current_dir().map_err(|error| {
            Error::ValidationError(format!("failed to resolve workspace: {error}"))
        })?;
        let (node_runtime, python_runtime) = runtime_configs()?;
        let script_path = scheduled_jobs_dir().join(&self.manifest.script);
        let request = json!({
            "task": {
                "id": context.task.id,
                "task_name": context.task.task_name,
                "source_service": context.task.source_service,
                "triggered_by": context.task.triggered_by,
                "start_time": context.task.start_time.to_rfc3339(),
            },
            "agent_id": context.resources.agent_id,
            "sender_id": context.task.triggered_by,
        });
        let resources = Arc::clone(&context.resources);
        let response = dynamic_script_engine::execute_script_tool(
            &workspace,
            self.language,
            &node_runtime,
            &python_runtime,
            &script_path,
            &self.manifest.entry,
            &request,
            &mut |method: &str, params: &Value| capabilities::dispatch(&resources, method, params),
        )
        .map_err(|error| crate::string_error!("job script 执行失败: {error}"))?;
        let ok = response
            .get("ok")
            .and_then(Value::as_bool)
            .ok_or_else(|| Error::ValidationError("job script 响应缺少 ok 布尔值".to_string()))?;
        if !ok {
            let error = response.get("error").and_then(Value::as_str).unwrap_or("未知错误");
            return Err(Error::ValidationError(format!("job script 执行失败: {error}")));
        }
        Ok(response.get("result").and_then(Value::as_str).map(str::to_string))
    }
}
