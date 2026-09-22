//! Writes script tool sources to disk so the dynamic script engine can load them.
//!
//! A script tool keeps its code inline in the configuration; the engine, however, imports a
//! file. Materializing bridges the two: the source is written under the application data
//! directory, and the returned path is what execution and manifest loading hand to the engine.

use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::graph::tool_spec::ScriptToolConfig;
use crate::system_config::application_data_dir;

/// Upper bound on a stored script. The source is part of the configuration document, so this
/// stays well below what a service config can reasonably carry.
const MAX_SCRIPT_BYTES: usize = 256 * 1024;

/// Directory holding every materialized script tool source.
pub fn script_tool_dir() -> PathBuf {
    application_data_dir().join("script_tools")
}

/// Validates one script tool configuration and reports what it would be written as.
pub fn validate_script_tool_config(tool_name: &str, config: &ScriptToolConfig) -> Result<()> {
    if config.source.trim().is_empty() {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 的脚本内容不能为空",
            tool_name.trim()
        )));
    }
    if config.source.len() > MAX_SCRIPT_BYTES {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 的脚本超过 {} KiB 上限",
            tool_name.trim(),
            MAX_SCRIPT_BYTES / 1024
        )));
    }
    if config.entry.trim().is_empty() {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 的脚本入口函数不能为空",
            tool_name.trim()
        )));
    }
    if !is_valid_entry_name(config.entry.trim()) {
        return Err(Error::ValidationError(format!(
            "工具 '{}' 的脚本入口 '{}' 不是合法的函数名",
            tool_name.trim(),
            config.entry.trim()
        )));
    }
    Ok(())
}

/// Writes `config`'s source to disk and returns the path the engine should load.
///
/// The file name is derived from `namespace` and a hash of the source, so editing a script
/// produces a new path instead of silently serving a module the runner already imported and
/// cached. Older revisions under the same namespace are removed as a side effect.
pub fn materialize_script_source(namespace: &str, config: &ScriptToolConfig) -> Result<PathBuf> {
    validate_script_tool_config(namespace, config)?;
    let directory = script_tool_dir();
    std::fs::create_dir_all(&directory)?;
    ensure_module_scope(&directory)?;
    let slug = slugify(namespace);
    let digest = content_digest(&config.source);
    let file_name = format!("{slug}-{digest}.{}", config.language.file_extension());
    let path = directory.join(file_name);
    if !path.is_file() {
        std::fs::write(&path, config.source.as_bytes())?;
    }
    remove_stale_revisions(&directory, &slug, &path);
    Ok(path)
}

/// Marks the script directory as an ES module scope.
///
/// Node otherwise has to guess the module kind of every script it imports and warns about the
/// ambiguity on stderr. The scripts are modules by contract (they export their entry), so the
/// guess is always the same one, and saying so once removes the warning and the reparse.
fn ensure_module_scope(directory: &std::path::Path) -> Result<()> {
    let marker = directory.join("package.json");
    if !marker.is_file() {
        std::fs::write(&marker, br#"{"type":"module"}"#)?;
    }
    Ok(())
}

/// Deletes previously materialized revisions of one tool, keeping the current file.
fn remove_stale_revisions(directory: &std::path::Path, slug: &str, keep: &std::path::Path) {
    let prefix = format!("{slug}-");
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path == keep {
            continue;
        }
        let is_revision = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&prefix));
        if is_revision && path.is_file() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Restricts an id to characters that are safe in a file name on every supported platform.
fn slugify(value: &str) -> String {
    let slug: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if slug.trim_matches('_').is_empty() {
        "tool".to_string()
    } else {
        slug
    }
}

fn content_digest(source: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn is_valid_entry_name(entry: &str) -> bool {
    let mut characters = entry.chars();
    match characters.next() {
        Some(first) if first.is_alphabetic() || first == '_' || first == '$' => {}
        _ => return false,
    }
    characters.all(|character| character.is_alphanumeric() || character == '_' || character == '$')
}
