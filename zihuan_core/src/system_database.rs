use std::sync::Mutex;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sqlx::{any::AnyPoolOptions, AnyPool};

use crate::error::Result;
use crate::storage::{ConnectionConfig, ConnectionKind, SqliteConnection};
use crate::system_config::{load_section, save_section, SystemConfigSection};

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

fn configured_connection() -> Result<String> {
    let settings = load_section::<SystemDatabaseSection>()?;
    if !settings.connection_id.is_empty() {
        return Ok(settings.connection_id);
    }
    let directory = crate::system_config::application_data_dir();
    std::fs::create_dir_all(&directory)?;
    let connection = crate::storage::upsert_connection(ConnectionConfig {
        id: String::new(),
        config_id: String::new(),
        name: "sqlite3数据库".to_string(),
        enabled: true,
        kind: ConnectionKind::Sqlite(SqliteConnection {
            path: directory.join("zihuan_settings.sqlite3").to_string_lossy().into_owned(),
        }),
        updated_at: chrono::Utc::now().to_rfc3339(),
    })?;
    let connection_id = connection.canonical_config_id().to_string();
    save_section::<SystemDatabaseSection>(&SystemDatabaseSettings {
        connection_id: connection_id.clone(),
    })?;
    Ok(connection_id)
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

pub fn initialize() -> Result<()> {
    with_database(async |_| Ok(()))
}

pub fn settings() -> Result<SystemDatabaseSettings> {
    with_database(async |database| {
        Ok(SystemDatabaseSettings {
            connection_id: database.connection_id.clone(),
        })
    })
}

pub fn change(connection_id: &str, migrate: bool) -> Result<SystemDatabaseSettings> {
    initialize()?;
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
