//! Plugin management for directory-owned dynamic script plugins.
//!
//! Unlike the infrastructure plugins tracked in `plugins.json`, these live as directories
//! under `<program dir>/plugins/<name>/`. This module exposes what the plugin page needs:
//! the manifest and declared dependencies, the source tree, single-file previews, removal,
//! and installing a plugin from an uploaded zip archive.

use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;

use zihuan_core::script_plugins::find_script_plugin;

use super::render_error;

/// Entry file names a plugin directory may use, in precedence order.
const ENTRY_FILES: [&str; 2] = ["plugin.py", "plugin.mjs"];

/// Preview size cap. Source plugins are small; anything larger is almost certainly not code a
/// human wants to read in a panel, and reading it whole would only waste memory.
const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

/// Upload cap for plugin archives.
const MAX_ARCHIVE_BYTES: usize = 32 * 1024 * 1024;

/// Directory and file names that belong to a plugin's local environment rather than its
/// source, and therefore stay out of the tree, previews and installed archives.
const IGNORED_DIRS: [&str; 4] = [".venv", "__pycache__", ".git", "node_modules"];
const IGNORED_FILES: [&str; 3] = ["uv.lock", ".python-version", ".DS_Store"];

#[derive(Serialize)]
struct FileNode {
    name: String,
    path: String,
    kind: &'static str,
    children: Vec<FileNode>,
}

#[derive(Serialize)]
struct PluginDetail {
    name: String,
    display_name: String,
    description: String,
    version: String,
    plugin_type: String,
    language: String,
    methods: Vec<String>,
    directory: String,
    dependencies: Vec<String>,
    config_fields: Vec<dynamic_script_engine::PluginConfigField>,
    files: Vec<FileNode>,
}

#[derive(Serialize)]
struct FilePreview {
    path: String,
    content: Option<String>,
    binary: bool,
    truncated: bool,
}

fn plugins_root() -> Result<PathBuf, String> {
    let workspace =
        std::env::current_dir().map_err(|error| format!("无法获取工作目录: {error}"))?;
    Ok(dynamic_script_engine::plugin_dir(&workspace))
}

fn normalized_dir_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.contains('\\')
        || name == ".."
    {
        return Err("插件名称无效".to_string());
    }
    Ok(name.to_string())
}

/// Resolves one plugin directory under `plugins/`, refusing anything that is not a direct,
/// existing child directory.
fn plugin_directory(name: &str) -> Result<PathBuf, String> {
    let directory = normalized_dir_name(name)?;
    let path = plugins_root()?.join(&directory);
    if !path.is_dir() {
        return Err(format!("插件目录不存在: {directory}"));
    }
    Ok(path)
}

fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

fn is_ignored_file(name: &str) -> bool {
    IGNORED_FILES.contains(&name)
}

fn read_tree(dir: &Path, base: &Path, depth: usize) -> Vec<FileNode> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut nodes = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let display = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if path.is_dir() {
            if is_ignored_dir(&name) {
                continue;
            }
            nodes.push(FileNode {
                name,
                path: display,
                kind: "dir",
                children: if depth < 8 {
                    read_tree(&path, base, depth + 1)
                } else {
                    Vec::new()
                },
            });
        } else if path.is_file() && !is_ignored_file(&name) {
            nodes.push(FileNode {
                name,
                path: display,
                kind: "file",
                children: Vec::new(),
            });
        }
    }
    // Directories first, then files, each alphabetically: the conventional file-tree order.
    nodes.sort_by(|left, right| {
        (left.kind != "dir", left.name.to_lowercase())
            .cmp(&(right.kind != "dir", right.name.to_lowercase()))
    });
    nodes
}

/// Reads the dependency list a plugin declares, from `pyproject.toml` or `requirements.txt`.
///
/// Both formats are scanned textually rather than parsed: the shape is a fixed key with a
/// list of quoted strings or a line list, and pulling in a TOML parser for this would be a
/// dependency the whole service pays for.
fn declared_dependencies(dir: &Path) -> Vec<String> {
    let mut dependencies = Vec::new();
    if let Ok(text) = fs::read_to_string(dir.join("pyproject.toml")) {
        dependencies.extend(parse_pyproject_dependencies(&text));
    }
    if let Ok(text) = fs::read_to_string(dir.join("requirements.txt")) {
        dependencies.extend(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('-'))
                .map(str::to_string),
        );
    }
    dependencies
}

fn parse_pyproject_dependencies(text: &str) -> Vec<String> {
    let Some(key) = text.find("dependencies") else {
        return Vec::new();
    };
    let rest = &text[key..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let mut dependencies = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in rest[open + 1..].chars() {
        match quote {
            Some(active) if ch == active => {
                let value = current.trim();
                if !value.is_empty() {
                    dependencies.push(value.to_string());
                }
                current.clear();
                quote = None;
            }
            Some(_) => current.push(ch),
            None if ch == ']' => break,
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None => {}
        }
    }
    dependencies
}

/// Joins a user-supplied relative path onto the plugin directory, refusing escapes.
fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.trim().is_empty() {
        return Err("文件路径不能为空".to_string());
    }
    let candidate = Path::new(relative);
    if candidate.is_absolute()
        || candidate
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("文件路径无效".to_string());
    }
    let resolved = base.join(candidate);
    let canonical_base =
        base.canonicalize().map_err(|error| format!("无法解析插件目录: {error}"))?;
    let canonical_file = resolved.canonicalize().map_err(|error| format!("文件不存在: {error}"))?;
    if !canonical_file.starts_with(&canonical_base) {
        return Err("文件路径无效".to_string());
    }
    Ok(canonical_file)
}

#[handler]
pub async fn get_script_plugin_detail(req: &mut Request, res: &mut Response) {
    let name = req.param::<String>("name").unwrap_or_default();
    let Some(entry) = find_script_plugin(&name) else {
        return render_error(res, StatusCode::NOT_FOUND, format!("插件 '{name}' 未安装"));
    };
    let directory = normalized_dir_name(&entry.directory).unwrap_or_else(|_| entry.name.clone());
    let path = match plugin_directory(&directory) {
        Ok(path) => path,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let files = read_tree(&path, &path, 0);
    res.render(Json(PluginDetail {
        name: entry.name,
        display_name: entry.display_name,
        description: entry.description,
        version: entry.version,
        plugin_type: entry.plugin_type,
        language: format!("{:?}", entry.language).to_lowercase(),
        methods: entry.methods,
        directory,
        dependencies: declared_dependencies(&path),
        config_fields: entry.config_fields,
        files,
    }));
}

#[handler]
pub async fn get_script_plugin_file(req: &mut Request, res: &mut Response) {
    let name = req.param::<String>("name").unwrap_or_default();
    let relative = req.query::<String>("path").unwrap_or_default();
    let Some(entry) = find_script_plugin(&name) else {
        return render_error(res, StatusCode::NOT_FOUND, format!("插件 '{name}' 未安装"));
    };
    let directory = normalized_dir_name(&entry.directory).unwrap_or_else(|_| entry.name.clone());
    let base = match plugin_directory(&directory) {
        Ok(path) => path,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let file = match safe_join(&base, &relative) {
        Ok(file) => file,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error),
    };
    if !file.is_file() {
        return render_error(res, StatusCode::NOT_FOUND, "文件不存在".to_string());
    }
    let metadata = match fs::metadata(&file) {
        Ok(metadata) => metadata,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error.to_string()),
    };
    let truncated = metadata.len() > MAX_PREVIEW_BYTES;
    let mut bytes = Vec::new();
    match fs::File::open(&file)
        .and_then(|handle| handle.take(MAX_PREVIEW_BYTES).read_to_end(&mut bytes).map(|_| ()))
    {
        Ok(()) => {}
        Err(error) => {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    }
    res.render(Json(match String::from_utf8(bytes) {
        Ok(content) => FilePreview {
            path: relative,
            content: Some(content),
            binary: false,
            truncated,
        },
        Err(_) => FilePreview {
            path: relative,
            content: None,
            binary: true,
            truncated,
        },
    }));
}

#[handler]
pub async fn uninstall_script_plugin(req: &mut Request, res: &mut Response) {
    let name = req.param::<String>("name").unwrap_or_default();
    let Some(entry) = find_script_plugin(&name) else {
        return render_error(res, StatusCode::NOT_FOUND, format!("插件 '{name}' 未安装"));
    };
    let directory = normalized_dir_name(&entry.directory).unwrap_or_else(|_| entry.name.clone());
    let path = match plugin_directory(&directory) {
        Ok(path) => path,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    if let Err(error) = fs::remove_dir_all(&path) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    zihuan_core::script_plugins::forget_script_plugin(&entry.name);
    res.render(Json(serde_json::json!({ "ok": true, "directory": directory })));
}

/// Strips one shared top-level directory from archive entry names.
///
/// Zips normally wrap their content in a folder named after the plugin; dropping it makes
/// the archive's root the plugin directory itself.
fn strip_common_root(entries: &[String]) -> Option<String> {
    let mut root: Option<&str> = None;
    for entry in entries {
        let mut parts = entry.split('/').filter(|part| !part.is_empty());
        let Some(first) = parts.next() else {
            continue;
        };
        if parts.next().is_none() && !entry.ends_with('/') {
            // A top-level file means the archive root is already the plugin directory.
            return None;
        }
        match root {
            None => root = Some(first),
            Some(existing) if existing != first => return None,
            Some(_) => {}
        }
    }
    root.map(str::to_string)
}

fn archive_entry_allowed(path: &Path) -> bool {
    path.components().all(|component| match component {
        Component::Normal(name) => {
            let name = name.to_string_lossy();
            !is_ignored_dir(&name) && !is_ignored_file(&name)
        }
        Component::CurDir => true,
        _ => false,
    })
}

#[handler]
pub async fn install_script_plugin_zip(req: &mut Request, res: &mut Response) {
    let form_data = match req.form_data().await {
        Ok(data) => data,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let Some(file) = form_data.files.get("file") else {
        return render_error(res, StatusCode::BAD_REQUEST, "缺少上传的 zip 文件".to_string());
    };
    let bytes = match fs::read(file.path()) {
        Ok(bytes) => bytes,
        Err(error) => {
            return render_error(res, StatusCode::BAD_REQUEST, format!("无法读取上传文件: {error}"))
        }
    };
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            format!("插件包超过 {} MB 上限", MAX_ARCHIVE_BYTES / 1024 / 1024),
        );
    }
    let archive = match zip::ZipArchive::new(Cursor::new(bytes)) {
        Ok(archive) => archive,
        Err(error) => {
            return render_error(
                res,
                StatusCode::BAD_REQUEST,
                format!("不是有效的 zip 文件: {error}"),
            )
        }
    };
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let root_prefix = strip_common_root(&names);
    let directory = match root_prefix.as_deref() {
        Some(root) => match normalized_dir_name(root) {
            Ok(name) => name,
            Err(_) => {
                return render_error(
                    res,
                    StatusCode::BAD_REQUEST,
                    "插件包根目录名称无效".to_string(),
                )
            }
        },
        None => {
            let stem = file
                .name()
                .and_then(|name| {
                    Path::new(name).file_stem().map(|stem| stem.to_string_lossy().to_string())
                })
                .unwrap_or_default();
            match normalized_dir_name(&stem) {
                Ok(name) => name,
                Err(_) => {
                    return render_error(
                        res,
                        StatusCode::BAD_REQUEST,
                        "无法从插件包推断插件名".to_string(),
                    )
                }
            }
        }
    };
    let plugin_root = match plugins_root() {
        Ok(root) => root,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let target = plugin_root.join(&directory);
    if target.exists() {
        return render_error(
            res,
            StatusCode::CONFLICT,
            format!("插件 '{directory}' 已存在，请先卸载或改用其他名称"),
        );
    }
    if let Err(error) = fs::create_dir_all(&target) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    let extracted = match extract_archive(archive, &target, root_prefix.as_deref()) {
        Ok(()) => true,
        Err(error) => {
            let _ = fs::remove_dir_all(&target);
            return render_error(res, StatusCode::BAD_REQUEST, error);
        }
    };
    let _ = extracted;
    if !ENTRY_FILES.iter().any(|entry| target.join(entry).is_file()) {
        let _ = fs::remove_dir_all(&target);
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            format!("插件包中缺少入口文件 {} 或 {}", ENTRY_FILES[0], ENTRY_FILES[1]),
        );
    }
    // Load the freshly installed plugin in the background thread that runs the script
    // runners, so dependency installation does not block the async worker.
    let result = tokio::task::spawn_blocking(
        zihuan_core::script_plugins::reload_script_plugin_catalog_with_diagnostics,
    )
    .await;
    let (installed, diagnostics) = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
        Err(error) => {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
        }
    };
    let warnings: Vec<String> = diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.message.contains(&directory))
        .map(|diagnostic| diagnostic.message)
        .collect();
    res.render(Json(serde_json::json!({
        "ok": true,
        "directory": directory,
        "plugins": installed.iter().map(|plugin| plugin.name.clone()).collect::<Vec<_>>(),
        "warnings": warnings,
    })));
}

fn extract_archive(
    mut archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    target: &Path,
    root_prefix: Option<&str>,
) -> Result<(), String> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(raw_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err("插件包包含非法路径".to_string());
        };
        let relative = match root_prefix {
            Some(root) => match raw_name.strip_prefix(root) {
                Ok(rest) => rest.to_path_buf(),
                Err(_) => continue,
            },
            None => raw_name,
        };
        if relative.as_os_str().is_empty() || !archive_entry_allowed(&relative) {
            continue;
        }
        let destination = target.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut buffer = Vec::new();
        entry.read_to_end(&mut buffer).map_err(|error| error.to_string())?;
        fs::write(&destination, buffer).map_err(|error| error.to_string())?;
    }
    Ok(())
}
