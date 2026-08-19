use std::fs;
use std::path::{Path, PathBuf};

use salvo::http::StatusCode;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use zihuan_core::system_config::{
    load_section, save_section, WorkspaceDirectoryHistory, WorkspaceDirectoryHistorySection,
};

const MAX_RECENT_DIRECTORIES: usize = 20;

#[derive(Debug, Serialize)]
struct DirectoryEntry {
    name: String,
    path: String,
}

#[derive(Debug, Serialize)]
struct BrowseDirectoriesResponse {
    current_path: Option<String>,
    parent_path: Option<String>,
    roots: Vec<DirectoryEntry>,
    directories: Vec<DirectoryEntry>,
    recent_directories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SelectDirectoryRequest {
    path: String,
}

#[handler]
pub async fn browse_workspace_directories(req: &mut Request, res: &mut Response) {
    let requested_path = req.query::<String>("path");
    match browse_directories(requested_path.as_deref()) {
        Ok(response) => res.render(Json(response)),
        Err(error) => render_directory_error(res, error),
    }
}

#[handler]
pub async fn select_workspace_directory(req: &mut Request, res: &mut Response) {
    let body: SelectDirectoryRequest = match req.parse_json::<SelectDirectoryRequest>().await {
        Ok(body) => body,
        Err(error) => return render_directory_error(res, DirectoryError::InvalidPath(error.to_string())),
    };
    let path = match normalize_directory(&body.path) {
        Ok(path) => path,
        Err(error) => return render_directory_error(res, error),
    };
    let path_text = path.to_string_lossy().to_string();
    match save_recent_directory(&path_text) {
        Ok(recent_directories) => res.render(Json(json!({ "path": path_text, "recent_directories": recent_directories }))),
        Err(error) => render_directory_error(res, error),
    }
}

fn browse_directories(requested_path: Option<&str>) -> Result<BrowseDirectoriesResponse, DirectoryError> {
    let current_path = match requested_path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => Some(normalize_directory(path)?),
        None => Some(default_browse_directory()?),
    };
    let directories = match &current_path {
        Some(path) => list_directories(path)?,
        None => Vec::new(),
    };
    let recent_directories = load_recent_directories()?;
    Ok(BrowseDirectoriesResponse {
        parent_path: current_path.as_deref().and_then(Path::parent).map(path_to_string),
        current_path: current_path.as_deref().map(path_to_string),
        roots: root_directories()?,
        directories,
        recent_directories,
    })
}

fn default_browse_directory() -> Result<PathBuf, DirectoryError> {
    let home_directory = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    let path = home_directory.unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });
    normalize_directory(&path_to_string(&path))
}

fn normalize_directory(raw_path: &str) -> Result<PathBuf, DirectoryError> {
    let path = raw_path.trim();
    if path.is_empty() {
        return Err(DirectoryError::InvalidPath("path must not be empty".to_string()));
    }
    let canonical = fs::canonicalize(path).map_err(|error| DirectoryError::Unreadable(path.to_string(), error.to_string()))?;
    if !canonical.is_dir() {
        return Err(DirectoryError::NotDirectory(path.to_string()));
    }
    Ok(canonical)
}

fn list_directories(path: &Path) -> Result<Vec<DirectoryEntry>, DirectoryError> {
    let entries = fs::read_dir(path).map_err(|error| DirectoryError::Unreadable(path_to_string(path), error.to_string()))?;
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| DirectoryError::Unreadable(path_to_string(path), error.to_string()))?;
        let entry_path = entry.path();
        if entry_path.is_dir() {
            directories.push(DirectoryEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: path_to_string(&entry_path),
            });
        }
    }
    directories.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    Ok(directories)
}

fn root_directories() -> Result<Vec<DirectoryEntry>, DirectoryError> {
    #[cfg(windows)]
    {
        let roots = (b'A'..=b'Z')
            .map(|letter| PathBuf::from(format!("{}:\\", letter as char)))
            .filter(|path| path.is_dir())
            .map(|path| DirectoryEntry { name: path_to_string(&path), path: path_to_string(&path) })
            .collect();
        Ok(roots)
    }
    #[cfg(not(windows))]
    {
        let root = PathBuf::from("/");
        Ok(vec![DirectoryEntry { name: "/".to_string(), path: path_to_string(&root) }])
    }
}

fn load_recent_directories() -> Result<Vec<String>, DirectoryError> {
    let history = load_section::<WorkspaceDirectoryHistorySection>()
        .map_err(|error| DirectoryError::Persistence(error.to_string()))?;
    Ok(valid_recent_directories(history.paths))
}

fn save_recent_directory(path: &str) -> Result<Vec<String>, DirectoryError> {
    let paths = updated_recent_directories(load_recent_directories()?, path);
    save_section::<WorkspaceDirectoryHistorySection>(&WorkspaceDirectoryHistory { paths: paths.clone() })
        .map_err(|error| DirectoryError::Persistence(error.to_string()))?;
    Ok(paths)
}

fn valid_recent_directories(paths: Vec<String>) -> Vec<String> {
    paths.into_iter().filter(|path| fs::read_dir(path).is_ok()).collect()
}

fn updated_recent_directories(mut paths: Vec<String>, path: &str) -> Vec<String> {
    paths.retain(|existing| existing != path);
    paths.insert(0, path.to_string());
    paths.truncate(MAX_RECENT_DIRECTORIES);
    paths
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

#[derive(Debug)]
enum DirectoryError {
    InvalidPath(String),
    NotDirectory(String),
    Unreadable(String, String),
    Persistence(String),
}

fn render_directory_error(res: &mut Response, error: DirectoryError) {
    let (status, message) = match error {
        DirectoryError::InvalidPath(message) => (StatusCode::BAD_REQUEST, message),
        DirectoryError::NotDirectory(path) => (StatusCode::BAD_REQUEST, format!("not a directory: {path}")),
        DirectoryError::Unreadable(path, error) => (StatusCode::BAD_REQUEST, format!("unable to access directory '{path}': {error}")),
        DirectoryError::Persistence(error) => (StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    res.status_code(status);
    res.render(Json(json!({ "error": message })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roots_are_available() {
        assert!(!root_directories().expect("roots").is_empty());
    }

    #[test]
    fn test_browsing_a_file_is_rejected() {
        let file = std::env::current_exe().expect("current executable");
        assert!(matches!(normalize_directory(&path_to_string(&file)), Err(DirectoryError::NotDirectory(_))));
    }

    #[test]
    fn test_browsing_the_current_directory_lists_directories() {
        let current = std::env::current_dir().expect("current directory");
        let response = browse_directories(Some(&path_to_string(&current))).expect("browse current directory");
        let canonical = path_to_string(&fs::canonicalize(current).expect("canonical current"));
        assert_eq!(response.current_path.as_deref(), Some(canonical.as_str()));
    }

    #[test]
    fn test_browsing_without_a_path_opens_the_default_directory() {
        let response = browse_directories(None).expect("browse default directory");
        let default_path = path_to_string(&default_browse_directory().expect("default directory"));
        assert_eq!(response.current_path.as_deref(), Some(default_path.as_str()));
    }

    #[test]
    fn test_recent_directories_are_deduplicated_and_limited() {
        let paths = (0..MAX_RECENT_DIRECTORIES).map(|index| format!("/workspace/{index}")).collect();
        let updated = updated_recent_directories(paths, "/workspace/4");
        assert_eq!(updated.len(), MAX_RECENT_DIRECTORIES);
        assert_eq!(updated.first().map(String::as_str), Some("/workspace/4"));
        assert_eq!(updated.iter().filter(|path| path.as_str() == "/workspace/4").count(), 1);
    }

    #[test]
    fn test_inaccessible_recent_directories_are_filtered() {
        let current = path_to_string(&std::env::current_dir().expect("current directory"));
        let filtered = valid_recent_directories(vec!["__zihuan_missing_directory__".to_string(), current.clone()]);
        assert_eq!(filtered, vec![current]);
    }
}
