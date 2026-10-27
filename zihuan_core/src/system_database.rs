use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{any::AnyPoolOptions, AnyPool};

use crate::error::Result;
use crate::storage::{ConnectionConfig, ConnectionKind, SqliteConnection};
use crate::system_config::{load_section, save_section, SystemConfigSection};

/// Serializes creation of the system database connection, so two callers cannot both observe an
/// empty `system_database` section and each create a connection for the same file.
static CONNECTION_LOCK: Mutex<()> = Mutex::new(());

/// Stores the SQLite file that backs the system database when no connection is configured.
const SETTINGS_DATABASE_FILE: &str = "zihuan_settings.sqlite3";
/// Names the SQLite connection created for the settings database when none is configured.
const DEFAULT_SQLITE_CONNECTION_NAME: &str = "sqlite3数据库";
/// Names the settings database connection when it is created outside the startup fallback.
const SYSTEM_DATABASE_CONNECTION_NAME: &str = "系统配置数据库";
/// Names of connections that represent the settings database automatically, and the only names
/// duplicate cleanup removes.
const AUTO_CREATED_CONNECTION_NAMES: &[&str] =
    &[DEFAULT_SQLITE_CONNECTION_NAME, SYSTEM_DATABASE_CONNECTION_NAME];
/// Retired names of the settings database connection; connections carrying any of them are
/// renamed on startup.
const LEGACY_SYSTEM_DATABASE_CONNECTION_NAMES: &[&str] = &["系统配置数据库（默认 SQLite）"];

/// Reports whether `name` is one automatically given to the settings database connection,
/// including retired names.
fn is_auto_created_connection_name(name: &str) -> bool {
    AUTO_CREATED_CONNECTION_NAMES.contains(&name)
        || LEGACY_SYSTEM_DATABASE_CONNECTION_NAMES.contains(&name)
}

static RUNTIME: Lazy<tokio::runtime::Runtime> =
    Lazy::new(|| tokio::runtime::Runtime::new().expect("system database runtime"));
static DATABASE: Mutex<Option<SystemDatabase>> = Mutex::new(None);

#[derive(Default, Serialize, Deserialize)]
pub struct SystemDatabaseSettings {
    #[serde(default)]
    pub connection_id: String,
}

pub struct SystemDatabaseSection;

impl SystemConfigSection for SystemDatabaseSection {
    const SECTION_KEY: &'static str = "system_database";
    type Value = SystemDatabaseSettings;
}

pub struct SystemDatabase {
    pub connection_id: String,
    pub pool: AnyPool,
    pub mysql: bool,
}

impl SystemDatabase {
    async fn open(connection_id: &str) -> Result<Self> {
        sqlx::any::install_default_drivers();
        let connections = crate::storage::load_connections()?;
        let connection = crate::storage::find_connection(&connections, connection_id)?;
        let (options, mysql) = match &connection.kind {
            ConnectionKind::Mysql(config) => (
                AnyPoolOptions::new()
                    .max_connections(config.max_connections)
                    .acquire_timeout(std::time::Duration::from_secs(config.acquire_timeout_secs)),
                Some(config.url.clone()),
            ),
            ConnectionKind::Sqlite(config) => {
                let path = config.path.clone();
                (
                    AnyPoolOptions::new().max_connections(1).after_connect(move |connection, _| {
                        let path = path.clone();
                        Box::pin(async move {
                            sqlx::query("ATTACH DATABASE ? AS zihuan_settings")
                                .bind(path)
                                .execute(&mut *connection)
                                .await?;
                            sqlx::query("PRAGMA zihuan_settings.journal_mode = WAL")
                                .execute(&mut *connection)
                                .await?;
                            sqlx::query("PRAGMA busy_timeout = 30000").execute(connection).await?;
                            Ok(())
                        })
                    }),
                    None,
                )
            }
            _ => return Err(crate::string_error!("系统配置数据库必须是关系型数据库")),
        };
        let is_mysql = mysql.is_some();
        let pool = options.connect(mysql.as_deref().unwrap_or("sqlite://?mode=rwc")).await?;
        let database = Self {
            connection_id: connection_id.to_string(),
            pool,
            mysql: is_mysql,
        };
        crate::migration::prepare(&database).await?;
        Ok(database)
    }
}

/// Resolves the default SQLite file used as the settings database and makes sure it can be
/// created there.
fn settings_database_path() -> Result<String> {
    let directory = crate::system_config::application_data_dir();
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(SETTINGS_DATABASE_FILE).to_string_lossy().into_owned())
}

/// Compares SQLite connection paths the way the platform does.
fn same_database_path(left: &str, right: &str) -> bool {
    if cfg!(target_os = "windows") {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Reports whether `connection` is a SQLite connection for the settings database at `path`.
fn is_settings_sqlite(connection: &ConnectionConfig, path: &str) -> bool {
    match &connection.kind {
        ConnectionKind::Sqlite(SqliteConnection { path: connection_path }) => {
            same_database_path(connection_path, path)
        }
        _ => false,
    }
}

/// Finds the SQLite connection that already represents the settings database, so an empty
/// `system_database` section reuses it instead of creating another connection for the same file.
fn find_settings_connection(path: &str) -> Result<Option<ConnectionConfig>> {
    let candidates = crate::storage::load_connections()?
        .into_iter()
        .filter(|connection| is_settings_sqlite(connection, path))
        .collect::<Vec<_>>();
    Ok(candidates
        .iter()
        .find(|connection| connection.enabled)
        .or_else(|| candidates.first())
        .cloned())
}

/// Creates the default SQLite connection for the settings database.
fn create_settings_connection(path: String) -> Result<ConnectionConfig> {
    crate::storage::upsert_connection(ConnectionConfig {
        id: String::new(),
        config_id: String::new(),
        name: DEFAULT_SQLITE_CONNECTION_NAME.to_string(),
        enabled: true,
        kind: ConnectionKind::Sqlite(SqliteConnection { path }),
        updated_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Resolves the connection id stored in `system_database`, creating the default SQLite connection
/// only when neither the section nor an existing connection for the settings file supplies one.
fn configured_connection() -> Result<String> {
    let _guard = CONNECTION_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let settings = load_section::<SystemDatabaseSection>()?;
    if !settings.connection_id.is_empty() {
        return Ok(settings.connection_id);
    }

    let path = settings_database_path()?;
    let connection_id = match find_settings_connection(&path)? {
        Some(connection) => connection.canonical_config_id().to_string(),
        None => create_settings_connection(path)?.canonical_config_id().to_string(),
    };
    save_section::<SystemDatabaseSection>(&SystemDatabaseSettings {
        connection_id: connection_id.clone(),
    })?;
    Ok(connection_id)
}

/// Recursively looks for `needle` as a string value inside `value`.
fn json_mentions(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text == needle,
        Value::Array(items) => items.iter().any(|item| json_mentions(item, needle)),
        Value::Object(object) => object.values().any(|item| json_mentions(item, needle)),
        _ => false,
    }
}

/// Renames connections that still carry the retired default name, so the connection list shows the
/// current name without the redundant suffix.
fn rename_legacy_settings_connections() -> Result<()> {
    for mut connection in crate::storage::load_connections()? {
        if !LEGACY_SYSTEM_DATABASE_CONNECTION_NAMES.contains(&connection.name.as_str()) {
            continue;
        }
        let config_id = connection.canonical_config_id().to_string();
        connection.name = SYSTEM_DATABASE_CONNECTION_NAME.to_string();
        connection.updated_at = chrono::Utc::now().to_rfc3339();
        crate::storage::upsert_connection(connection)?;
        log::info!(
            "[system_database] renamed settings connection config_id={config_id} to '{SYSTEM_DATABASE_CONNECTION_NAME}'"
        );
    }
    Ok(())
}

/// Removes the SQLite connections that earlier releases accumulated for the settings database
/// while the `system_database` section was missing, leaving the referenced connection in place.
fn remove_duplicate_settings_connections() -> Result<()> {
    let settings = load_section::<SystemDatabaseSection>()?;
    if settings.connection_id.is_empty() {
        return Ok(());
    }

    let path = crate::system_config::application_data_dir()
        .join(SETTINGS_DATABASE_FILE)
        .to_string_lossy()
        .into_owned();
    let connections = crate::storage::load_connections()?;
    let Some(referenced) = connections
        .iter()
        .find(|connection| connection.canonical_config_id() == settings.connection_id)
    else {
        return Ok(());
    };
    // Only the default SQLite file accumulates duplicates; a system database hosted elsewhere
    // leaves the connections for that file untouched.
    if !is_settings_sqlite(referenced, &path) {
        return Ok(());
    }

    let duplicates = connections
        .iter()
        .filter(|connection| {
            connection.canonical_config_id() != settings.connection_id
                && is_settings_sqlite(connection, &path)
                && is_auto_created_connection_name(&connection.name)
        })
        .cloned()
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        return Ok(());
    }

    let root = crate::config::ConfigCenter::shared().load_root()?;
    let referenced_by_config = |config_id: &str| {
        root.configs
            .agents
            .iter()
            .chain(root.configs.llm_refs.iter())
            .any(|record| json_mentions(&record.spec, config_id))
    };
    for duplicate in duplicates {
        let config_id = duplicate.canonical_config_id();
        if referenced_by_config(config_id) {
            log::warn!(
                "[system_database] keeping duplicate SQLite connection config_id={config_id} because other configuration references it"
            );
            continue;
        }
        crate::storage::delete_connection(config_id)?;
        log::info!(
            "[system_database] removed duplicate SQLite connection config_id={config_id} name='{}'",
            duplicate.name
        );
    }
    Ok(())
}

pub fn with_database<T>(operation: impl AsyncFnOnce(&SystemDatabase) -> Result<T>) -> Result<T> {
    let mut database = DATABASE
        .lock()
        .map_err(|_| crate::string_error!("system database lock poisoned"))?;
    RUNTIME.block_on(async {
        if database.is_none() {
            let opened = SystemDatabase::open(&configured_connection()?).await?;
            crate::migration::import_legacy_usage(&opened).await?;
            *database = Some(opened);
        }
        operation(database.as_ref().unwrap()).await
    })
}

/// Opens the system database on first use without touching the persisted configuration.
fn ensure_open() -> Result<()> {
    with_database(async |_| Ok(()))
}

pub fn initialize() -> Result<()> {
    ensure_open()?;
    if let Err(error) = rename_legacy_settings_connections() {
        log::warn!("[system_database] failed to rename legacy settings connections: {error}");
    }
    if let Err(error) = remove_duplicate_settings_connections() {
        log::warn!("[system_database] failed to remove duplicate settings connections: {error}");
    }
    Ok(())
}

pub fn settings() -> Result<SystemDatabaseSettings> {
    with_database(async |database| {
        Ok(SystemDatabaseSettings {
            connection_id: database.connection_id.clone(),
        })
    })
}

pub fn change(connection_id: &str, migrate: bool) -> Result<SystemDatabaseSettings> {
    ensure_open()?;
    let mut current = DATABASE
        .lock()
        .map_err(|_| crate::string_error!("system database lock poisoned"))?;
    RUNTIME.block_on(async {
        if current.as_ref().unwrap().connection_id != connection_id {
            let target = SystemDatabase::open(connection_id).await?;
            if migrate {
                crate::migration::migrate(current.as_ref().unwrap(), &target).await?;
            }
            crate::migration::skip_legacy_usage(&target).await?;
            save_section::<SystemDatabaseSection>(&SystemDatabaseSettings {
                connection_id: connection_id.to_string(),
            })?;
            *current = Some(target);
        }
        Ok(SystemDatabaseSettings { connection_id: connection_id.to_string() })
    })
}
