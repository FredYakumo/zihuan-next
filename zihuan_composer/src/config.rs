use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{de::DeserializeOwned, Serialize};

static ROOT: OnceLock<PathBuf> = OnceLock::new();
pub static WRITE_LOCK: Mutex<()> = Mutex::new(());

tokio::task_local! {
    pub static COMPONENT_SCOPE: (String, PathBuf);
}

pub fn initialize(path: PathBuf) -> Result<fs::File, String> {
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path.join("composer.lock"))
        .map_err(|error| error.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock)
        .map_err(|_| "此数据目录已由另一个 Composer 进程使用".to_string())?;
    ROOT.set(path).map_err(|_| "Composer 已初始化".to_string())?;
    Ok(lock)
}

pub fn root() -> PathBuf {
    ROOT.get().expect("Composer data directory initialized").clone()
}

pub fn data_dir() -> PathBuf {
    COMPONENT_SCOPE.try_with(|scope| scope.1.clone()).unwrap_or_else(|_| root())
}

pub fn connection_id(id: &str) -> String {
    COMPONENT_SCOPE
        .try_with(|scope| format!("{}-{id}", scope.0))
        .unwrap_or_else(|_| id.to_string())
}

pub fn read_yaml<Value: DeserializeOwned + Default>(path: &Path) -> Result<Value, String> {
    match fs::read_to_string(path) {
        Ok(content) => {
            serde_yaml::from_str(&content).map_err(|error| format!("{}: {error}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::default()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

pub fn write_yaml<Value: Serialize + ?Sized>(path: &Path, value: &Value) -> Result<(), String> {
    let _guard = WRITE_LOCK.lock().map_err(|error| error.to_string())?;
    write_yaml_unlocked(path, value)
}

pub fn write_yaml_unlocked<Value: Serialize + ?Sized>(
    path: &Path,
    value: &Value,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_yaml::to_string(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension("yaml.tmp");
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}
