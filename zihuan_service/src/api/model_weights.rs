use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use futures_util::StreamExt;
use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::task;
use uuid::Uuid;
use zihuan_core::model_inference::nn::llm_registry::list_local_llm_models;
use zihuan_core::system_config::model_download_endpoint;

const TEXT_EMBEDDING_MODEL_DIR: &str = "models/text_embedding";
const LLM_MODEL_DIR: &str = "models/llm";

/// Files that make up a complete local text-embedding model directory.
const EMBEDDING_MODEL_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

#[derive(Debug, Clone, Copy, PartialEq)]
enum WeightKind {
    TextEmbedding,
    Llm,
}

impl WeightKind {
    fn parse(kind: &str) -> Option<Self> {
        match kind {
            "text_embedding" => Some(Self::TextEmbedding),
            "llm" => Some(Self::Llm),
            _ => None,
        }
    }

    fn dir(self) -> &'static str {
        match self {
            Self::TextEmbedding => TEXT_EMBEDDING_MODEL_DIR,
            Self::Llm => LLM_MODEL_DIR,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TextEmbedding => "文本嵌入模型",
            Self::Llm => "LLM",
        }
    }
}

/// Rejects names that could escape the model root (empty, path separators, traversal).
fn is_safe_model_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && Path::new(name).file_name().is_some_and(|n| n == name)
}

fn resolve_model_dir(kind: WeightKind, name: &str) -> Option<PathBuf> {
    if !is_safe_model_name(name) {
        return None;
    }
    Some(Path::new(kind.dir()).join(name))
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            if p.is_file() {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            } else if p.is_dir() {
                dir_size(&p)
            } else {
                0
            }
        })
        .sum()
}

fn abs_path_str(path: &Path) -> String {
    let s = path
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    // Windows canonicalize() prepends \\?\ (extended-length path prefix) — strip it.
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

#[derive(Serialize)]
pub struct ModelWeightEntry {
    pub name: String,
    pub path: String,
    pub valid: bool,
    pub size_bytes: u64,
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct ModelWeightGroup {
    pub kind: String,
    pub label: String,
    pub dir: String,
    pub models: Vec<ModelWeightEntry>,
}

fn scan_text_embedding_group() -> ModelWeightGroup {
    let dir = Path::new(TEXT_EMBEDDING_MODEL_DIR);
    let models = match std::fs::read_dir(dir) {
        Ok(entries) => {
            let mut models: Vec<ModelWeightEntry> = entries
                .filter_map(|e| e.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    if !path.is_dir() {
                        return None;
                    }
                    let name = path.file_name()?.to_str()?.to_string();
                    let missing: Vec<&str> = EMBEDDING_MODEL_FILES
                        .iter()
                        .copied()
                        .filter(|f| !path.join(f).is_file())
                        .collect();
                    let valid = missing.is_empty();
                    let size_bytes = if valid { dir_size(&path) } else { 0 };
                    let detail = (!valid).then(|| {
                        format!("缺少文件: {}", missing.join(", "))
                    });
                    Some(ModelWeightEntry {
                        name,
                        path: abs_path_str(&path),
                        valid,
                        size_bytes,
                        detail,
                    })
                })
                .collect();
            models.sort_by(|a, b| a.name.cmp(&b.name));
            models
        }
        Err(_) => Vec::new(),
    };
    ModelWeightGroup {
        kind: "text_embedding".to_string(),
        label: WeightKind::TextEmbedding.label().to_string(),
        dir: abs_path_str(dir),
        models,
    }
}

/// Serializes a serde enum to its snake_case string form (e.g. `Gguf` → "gguf").
fn serde_enum_str<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn scan_llm_group() -> ModelWeightGroup {
    let dir = Path::new(LLM_MODEL_DIR);
    let models = list_local_llm_models()
        .into_iter()
        .map(|info| {
            let path = dir.join(&info.model_name);
            let size_bytes = if path.is_dir() { dir_size(&path) } else { 0 };
            let mut detail = format!("{}/{}", serde_enum_str(&info.kind), serde_enum_str(&info.layout));
            if let Some(reason) = &info.reason {
                detail.push_str(&format!(" - {reason}"));
            }
            ModelWeightEntry {
                name: info.model_name,
                path: abs_path_str(&path),
                valid: info.available,
                size_bytes,
                detail: Some(detail),
            }
        })
        .collect();
    ModelWeightGroup {
        kind: "llm".to_string(),
        label: WeightKind::Llm.label().to_string(),
        dir: abs_path_str(dir),
        models,
    }
}

#[handler]
pub async fn list_model_weights(_req: &mut Request, res: &mut Response) {
    let groups = task::spawn_blocking(|| vec![scan_text_embedding_group(), scan_llm_group()])
        .await
        .unwrap_or_default();
    res.render(Json(serde_json::json!({ "groups": groups })));
}

#[handler]
pub async fn delete_model_weight(req: &mut Request, res: &mut Response) {
    let kind = req.param::<String>("kind").unwrap_or_default();
    let name = req.param::<String>("name").unwrap_or_default();
    let Some(kind) = WeightKind::parse(&kind) else {
        return render_error(res, StatusCode::BAD_REQUEST, format!("未知的模型目录类型: {kind}"));
    };
    let Some(path) = resolve_model_dir(kind, &name) else {
        return render_error(res, StatusCode::BAD_REQUEST, "非法的模型目录名".to_string());
    };
    if !path.is_dir() {
        return render_error(res, StatusCode::NOT_FOUND, "模型目录不存在".to_string());
    }
    match tokio::fs::remove_dir_all(&path).await {
        Ok(()) => res.render(Json(serde_json::json!({ "ok": true }))),
        Err(error) => render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("删除模型目录失败: {error}"),
        ),
    }
}

#[derive(Deserialize)]
pub struct RenameModelWeightRequest {
    pub new_name: String,
}

#[handler]
pub async fn rename_model_weight(req: &mut Request, res: &mut Response) {
    let kind = req.param::<String>("kind").unwrap_or_default();
    let name = req.param::<String>("name").unwrap_or_default();
    let Some(kind) = WeightKind::parse(&kind) else {
        return render_error(res, StatusCode::BAD_REQUEST, format!("未知的模型目录类型: {kind}"));
    };
    let Some(path) = resolve_model_dir(kind, &name) else {
        return render_error(res, StatusCode::BAD_REQUEST, "非法的模型目录名".to_string());
    };
    let body: RenameModelWeightRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let new_name = body.new_name.trim().to_string();
    let Some(new_path) = resolve_model_dir(kind, &new_name) else {
        return render_error(res, StatusCode::BAD_REQUEST, "非法的新模型目录名".to_string());
    };
    if !path.is_dir() {
        return render_error(res, StatusCode::NOT_FOUND, "模型目录不存在".to_string());
    }
    if new_path.exists() {
        return render_error(res, StatusCode::CONFLICT, "目标目录名已存在".to_string());
    }
    match tokio::fs::rename(&path, &new_path).await {
        Ok(()) => res.render(Json(serde_json::json!({ "ok": true, "name": new_name }))),
        Err(error) => render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("重命名模型目录失败: {error}"),
        ),
    }
}

#[derive(Deserialize)]
pub struct ImportModelWeightRequest {
    pub kind: String,
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target = dst.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &target)?;
        } else {
            std::fs::copy(&entry_path, &target)?;
        }
    }
    Ok(())
}

/// Opens a native folder picker, copies the chosen directory into the models root
/// of `kind`, and returns the imported model name.
#[handler]
pub async fn import_model_weight(req: &mut Request, res: &mut Response) {
    let body: ImportModelWeightRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let Some(kind) = WeightKind::parse(&body.kind) else {
        return render_error(res, StatusCode::BAD_REQUEST, format!("未知的模型目录类型: {}", body.kind));
    };

    let picked = task::spawn_blocking(|| {
        tinyfiledialogs::select_folder_dialog("选择模型权重目录", "")
    })
    .await
    .unwrap_or(None);
    let Some(source) = picked else {
        res.render(Json(serde_json::json!({ "cancelled": true })));
        return;
    };
    let source = PathBuf::from(source.trim());
    let Some(folder_name) = source.file_name().and_then(|n| n.to_str()) else {
        return render_error(res, StatusCode::BAD_REQUEST, "无法从所选路径解析目录名".to_string());
    };
    let Some(dest) = resolve_model_dir(kind, folder_name) else {
        return render_error(res, StatusCode::BAD_REQUEST, "所选目录名不合法".to_string());
    };
    if dest.exists() {
        return render_error(
            res,
            StatusCode::CONFLICT,
            format!("同名模型目录已存在: {}", dest.display()),
        );
    }
    if !source.is_dir() {
        return render_error(res, StatusCode::BAD_REQUEST, "所选路径不是目录".to_string());
    }

    let source_clone = source.clone();
    let result = task::spawn_blocking(move || copy_dir_recursive(&source_clone, &dest))
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string())));
    match result {
        Ok(()) => res.render(Json(serde_json::json!({
            "cancelled": false,
            "name": folder_name,
        }))),
        Err(error) => {
            let _ = tokio::fs::remove_dir_all(resolve_model_dir(kind, folder_name).unwrap_or_default()).await;
            render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("复制模型目录失败: {error}"),
            )
        }
    }
}

#[derive(Deserialize)]
pub struct OpenModelWeightDirRequest {
    pub kind: String,
}

/// Opens the model weight directory of `kind` in the OS file manager.
#[handler]
pub async fn open_model_weight_dir(req: &mut Request, res: &mut Response) {
    let body: OpenModelWeightDirRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let Some(kind) = WeightKind::parse(&body.kind) else {
        return render_error(res, StatusCode::BAD_REQUEST, format!("未知的模型目录类型: {}", body.kind));
    };
    let dir = PathBuf::from(kind.dir());
    if let Err(error) = tokio::fs::create_dir_all(&dir).await {
        return render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("创建模型目录失败: {error}"),
        );
    }
    let opened = task::spawn_blocking(move || {
        #[cfg(target_os = "macos")]
        return std::process::Command::new("open").arg(&dir).spawn();
        #[cfg(target_os = "windows")]
        return std::process::Command::new("explorer").arg(&dir).spawn();
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return std::process::Command::new("xdg-open").arg(&dir).spawn();
    })
    .await
    .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string())));
    match opened {
        Ok(_) => res.render(Json(serde_json::json!({ "ok": true }))),
        Err(error) => render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("打开本地目录失败: {error}"),
        ),
    }
}

// ─── Model weight download (e.g. HuggingFace embedding models) ─────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DownloadJob {
    pub id: String,
    pub repo_id: String,
    pub dest_dir: String,
    /// queued | downloading | completed | failed | cancelled
    pub status: String,
    pub current_file: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
    #[serde(skip_serializing)]
    pub cancel_requested: bool,
}

static DOWNLOAD_JOBS: LazyLock<Mutex<HashMap<String, DownloadJob>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn update_job(job_id: &str, update: impl FnOnce(&mut DownloadJob)) {
    let mut jobs = DOWNLOAD_JOBS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(job) = jobs.get_mut(job_id) {
        update(job);
    }
}

fn is_cancel_requested(job_id: &str) -> bool {
    let jobs = DOWNLOAD_JOBS.lock().unwrap_or_else(|p| p.into_inner());
    jobs.get(job_id).map(|job| job.cancel_requested).unwrap_or(false)
}

#[derive(Deserialize)]
pub struct DownloadModelWeightRequest {
    pub repo_id: String,
    #[serde(default)]
    pub kind: Option<String>,
    /// GGUF weight file inside `repo_id` (LLM downloads only). When omitted, the repo's
    /// single `*.gguf` file is used automatically.
    #[serde(default)]
    pub file: Option<String>,
    /// Repo providing tokenizer.json (LLM downloads only). Defaults to `repo_id` with the
    /// "-GGUF" suffix stripped.
    #[serde(default)]
    pub tokenizer_repo: Option<String>,
}

/// Lists the GGUF files of a HuggingFace repo through its API, returning the single weight
/// file when unambiguous.
async fn resolve_single_gguf_file(endpoint: &str, repo_id: &str) -> Result<String, String> {
    let url = format!("{endpoint}/api/models/{repo_id}");
    let response = reqwest::get(&url)
        .await
        .map_err(|error| format!("查询仓库文件失败: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("查询仓库文件失败: HTTP {}", response.status()));
    }
    let payload: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("解析仓库文件列表失败: {error}"))?;
    let files: Vec<String> = payload
        .get("siblings")
        .and_then(|siblings| siblings.as_array())
        .map(|siblings| {
            siblings
                .iter()
                .filter_map(|s| s.get("rfilename").and_then(|f| f.as_str()))
                .filter(|f| f.to_ascii_lowercase().ends_with(".gguf"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    match files.len() {
        1 => Ok(files[0].clone()),
        0 => Err("仓库内未找到 .gguf 权重文件".to_string()),
        _ => Err(format!(
            "仓库内存在多个 .gguf 文件，请指定要下载的文件: {}",
            files.join(", ")
        )),
    }
}

#[handler]
pub async fn download_model_weights(req: &mut Request, res: &mut Response) {
    let body: DownloadModelWeightRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let Some(kind) = body.kind.as_deref().or(Some("text_embedding")).and_then(WeightKind::parse) else {
        let kind = body.kind.as_deref().unwrap_or("text_embedding");
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            format!("未知的模型目录类型: {kind}"),
        );
    };

    let repo_id = body.repo_id.trim().trim_end_matches('/').to_string();
    let segments: Vec<&str> = repo_id.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() != 2 || !segments.iter().all(|s| is_safe_model_name(s)) {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            "repo id 需形如 'Owner/Model'，例如 Qwen/Qwen3-Embedding-0.6B".to_string(),
        );
    }
    let repo_id = segments.join("/");
    let endpoint = model_download_endpoint();

    // Resolves (model_name, files) where files are (filename, source repo) download pairs.
    let (model_name, files) = match kind {
        WeightKind::TextEmbedding => {
            let model_name = segments[1].to_string();
            let files = EMBEDDING_MODEL_FILES
                .iter()
                .map(|f| (f.to_string(), repo_id.clone()))
                .collect();
            (model_name, files)
        }
        WeightKind::Llm => {
            let weight_file = match &body.file {
                Some(file) => {
                    let file = file.trim().to_string();
                    if !is_safe_model_name(&file) || !file.to_ascii_lowercase().ends_with(".gguf") {
                        return render_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            format!("非法的 GGUF 权重文件名: {file}"),
                        );
                    }
                    file
                }
                None => match resolve_single_gguf_file(&endpoint, &repo_id).await {
                    Ok(file) => file,
                    Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error),
                },
            };
            let model_name = weight_file
                .strip_suffix(".gguf")
                .or_else(|| Path::new(&weight_file).file_stem().and_then(|s| s.to_str()))
                .unwrap_or(&weight_file)
                .to_string();
            if !is_safe_model_name(&model_name) {
                return render_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    format!("无法从权重文件名解析模型目录名: {weight_file}"),
                );
            }
            let tokenizer_repo = body
                .tokenizer_repo
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| repo_id.strip_suffix("-GGUF").unwrap_or(&repo_id).to_string());
            let files = vec![
                ("tokenizer.json".to_string(), tokenizer_repo),
                (weight_file, repo_id.clone()),
            ];
            (model_name, files)
        }
    };

    let dest_dir = Path::new(kind.dir()).join(&model_name);
    if dest_dir.exists() {
        return render_error(
            res,
            StatusCode::CONFLICT,
            format!("本地已存在同名模型目录: {model_name}"),
        );
    }

    let job_id = Uuid::new_v4().to_string();
    let job = DownloadJob {
        id: job_id.clone(),
        repo_id: repo_id.clone(),
        dest_dir: abs_path_str(&dest_dir),
        status: "queued".to_string(),
        current_file: None,
        downloaded_bytes: 0,
        total_bytes: 0,
        error: None,
        cancel_requested: false,
    };
    DOWNLOAD_JOBS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(job_id.clone(), job);

    tokio::spawn(run_download(job_id, files, dest_dir));
    res.render(Json(serde_json::json!({ "ok": true, "model_name": model_name })));
}

/// Downloads each `(filename, source repo)` pair into `dest_dir`, streaming with progress
/// and cancellation support.
async fn run_download(job_id: String, files: Vec<(String, String)>, dest_dir: PathBuf) {
    let finish = |status: &str, error: Option<String>| {
        update_job(&job_id, |job| {
            job.status = status.to_string();
            job.error = error.clone();
        });
    };

    let cancel = || async {
        finish("cancelled", None);
        let _ = tokio::fs::remove_dir_all(&dest_dir).await;
    };

    if is_cancel_requested(&job_id) {
        cancel().await;
        return;
    }
    if let Err(error) = tokio::fs::create_dir_all(&dest_dir).await {
        finish("failed", Some(format!("创建模型目录失败: {error}")));
        return;
    }

    let endpoint = model_download_endpoint();
    let client = match reqwest::Client::builder().build() {
        Ok(client) => client,
        Err(error) => {
            finish("failed", Some(format!("创建 HTTP 客户端失败: {error}")));
            let _ = tokio::fs::remove_dir_all(&dest_dir).await;
            return;
        }
    };

    for (file, file_repo) in files {
        if is_cancel_requested(&job_id) {
            cancel().await;
            return;
        }
        let url = format!("{endpoint}/{file_repo}/resolve/main/{file}");
        update_job(&job_id, |job| {
            job.status = "downloading".to_string();
            job.current_file = Some(file.to_string());
        });

        let mut response = match client.get(&url).send().await {
            Ok(response) => response,
            Err(error) => {
                finish("failed", Some(format!("下载 {file} 失败: {error}")));
                let _ = tokio::fs::remove_dir_all(&dest_dir).await;
                return;
            }
        };
        if !response.status().is_success() {
            finish(
                "failed",
                Some(format!("下载 {file} 失败: HTTP {}", response.status())),
            );
            let _ = tokio::fs::remove_dir_all(&dest_dir).await;
            return;
        }
        let file_total = response.content_length().unwrap_or(0);
        update_job(&job_id, |job| job.total_bytes += file_total);

        let dest_file = dest_dir.join(&file);
        let mut writer = match tokio::fs::File::create(&dest_file).await {
            Ok(writer) => writer,
            Err(error) => {
                finish("failed", Some(format!("写入 {file} 失败: {error}")));
                let _ = tokio::fs::remove_dir_all(&dest_dir).await;
                return;
            }
        };
        let mut stream = response.bytes_stream();
        loop {
            if is_cancel_requested(&job_id) {
                cancel().await;
                return;
            }
            match stream.next().await {
                Some(Ok(chunk)) => {
                    if let Err(error) = writer.write_all(&chunk).await {
                        finish("failed", Some(format!("写入 {file} 失败: {error}")));
                        let _ = tokio::fs::remove_dir_all(&dest_dir).await;
                        return;
                    }
                    update_job(&job_id, |job| job.downloaded_bytes += chunk.len() as u64);
                }
                Some(Err(error)) => {
                    finish("failed", Some(format!("下载 {file} 失败: {error}")));
                    let _ = tokio::fs::remove_dir_all(&dest_dir).await;
                    return;
                }
                None => break,
            }
        }
        if let Err(error) = writer.flush().await {
            finish("failed", Some(format!("写入 {file} 失败: {error}")));
            let _ = tokio::fs::remove_dir_all(&dest_dir).await;
            return;
        }
    }

    update_job(&job_id, |job| {
        job.status = "completed".to_string();
        job.current_file = None;
    });
}

#[handler]
pub async fn cancel_download_job(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let job = {
        let mut jobs = DOWNLOAD_JOBS.lock().unwrap_or_else(|p| p.into_inner());
        let Some(job) = jobs.get_mut(&id) else {
            return render_error(res, StatusCode::NOT_FOUND, "下载任务不存在".to_string());
        };
        if job.status == "queued" || job.status == "downloading" {
            job.cancel_requested = true;
        }
        job.clone()
    };
    res.render(Json(serde_json::json!({ "ok": true, "status": job.status })));
}

#[handler]
pub async fn list_download_jobs(_req: &mut Request, res: &mut Response) {
    let mut jobs: Vec<DownloadJob> = {
        let jobs = DOWNLOAD_JOBS.lock().unwrap_or_else(|p| p.into_inner());
        jobs.values().cloned().collect()
    };
    jobs.sort_by(|a, b| b.id.cmp(&a.id));
    res.render(Json(serde_json::json!({ "jobs": jobs })));
}

fn render_error(res: &mut Response, status: StatusCode, error: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": error })));
}
