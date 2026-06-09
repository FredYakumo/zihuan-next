mod api;
mod error;
mod init_registry;
mod log_forwarder;
mod setup_orchestrator;
mod system_config;
mod util;

use std::sync::Arc;

use clap::Parser;
use lazy_static::lazy_static;
use log::{error, info};
use log_util::log_util::LogUtil;
use salvo::Listener;
use sqlx;
use zihuan_core::config::ConfigRepository;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next", "logs");
}

#[derive(Parser, Debug)]
#[command(author, version, about = "Zihuan Next — node-graph workflow engine (web UI)")]
struct Args {
    #[arg(long, default_value = "127.0.0.1", env = "ZIHUAN_HOST")]
    host: String,

    #[arg(long, default_value_t = 9951, env = "ZIHUAN_PORT")]
    port: u16,
}

#[tokio::main]
async fn main() {
    log_forwarder::init(&BASE_LOG);

    if let Err(e) = init_registry::init_node_registry() {
        error!("Failed to initialize node registry: {}", e);
    } else {
        info!("Node registry initialized");
    }

    // Initialize global command registry and sync persisted permissions
    {
        let registry = zihuan_service::command::init_global_command_registry();
        info!("Command registry initialized with {} commands", registry.list_commands().len());

        // Load persisted permissions from config.yaml and apply to registry
        let repo = zihuan_core::config::FsConfigRepository::default();
        if let Ok(root) = repo.load_root() {
            for record in &root.configs.command_permissions {
                if let Ok(cmd) = serde_json::from_value::<zihuan_core::command::CommandPermission>(record.spec.clone())
                {
                    registry.set_permissions(&cmd.command_name, cmd.rules);
                }
            }
        }
    }

    // Ensure database tables exist for all existing MySQL/SQLite connections.
    ensure_database_tables_for_existing_connections().await;

    let args = Args::parse();

    let state = Arc::new(api::state::AppState::new());
    let broadcast = api::ws::create_broadcast();
    log_forwarder::set_app_state(Arc::clone(&state));
    log_forwarder::set_broadcast(broadcast.clone());

    register_existing_relational_db_pools(&state).await;
    startup_recover_orphan_tasks(&state).await;
    spawn_task_ttl_cleanup(Arc::clone(&state));

    {
        let agents = crate::system_config::load_agents().unwrap_or_else(|e| {
            error!("Failed to load agents for auto start: {e}");
            Vec::new()
        });
        let connections = crate::system_config::load_connections().unwrap_or_else(|e| {
            error!("Failed to load connections for auto start: {e}");
            Vec::new()
        });
        for agent in agents.into_iter().filter(|a| a.enabled && a.auto_start) {
            if let Err(err) = api::config::agents::start_agent_runtime(
                Arc::clone(&state),
                broadcast.clone(),
                agent.clone(),
                connections.clone(),
            )
            .await
            {
                error!("Failed to auto start agent '{}' (id={}): {}", agent.name, agent.id, err);
            }
        }
    }

    let listen_addr = format!("{}:{}", args.host, args.port);
    let display_addr = match args.host.as_str() {
        "0.0.0.0" => format!("127.0.0.1:{}", args.port),
        "::" => format!("[::1]:{}", args.port),
        _ => listen_addr.clone(),
    };
    let canonical_local_origin = match args.host.as_str() {
        "127.0.0.1" => Some(format!("http://127.0.0.1:{}", args.port)),
        "::1" => Some(format!("http://[::1]:{}", args.port)),
        "localhost" => Some(format!("http://localhost:{}", args.port)),
        _ => None,
    };
    info!("Starting web server on http://{}", listen_addr);
    info!("Open your browser at  http://{}", display_addr);

    let router = api::build_router(Arc::clone(&state), broadcast, canonical_local_origin);
    let service = salvo::Service::new(router);
    let acceptor = salvo::conn::TcpListener::new(&listen_addr)
        .try_bind()
        .await
        .expect("Failed to bind TCP listener");
    salvo::Server::new(acceptor).serve(service).await;
}

async fn startup_recover_orphan_tasks(state: &api::state::AppState) {
    let pools: Vec<(String, zihuan_core::data_refs::RelationalDbConnection)> =
        state.tasks.lock().unwrap().all_db_pools().into_iter().collect();

    for (conn_id, pool) in pools {
        match api::task_store::mark_orphan_running_stopped(&pool).await {
            Ok(count) if count > 0 => {
                info!("Recovered {} orphan running tasks for connection '{}'", count, conn_id);
            }
            Err(err) => {
                log::warn!("Failed to recover orphan tasks for connection '{}': {}", conn_id, err);
            }
            _ => {}
        }
    }
}

async fn ensure_database_tables_for_existing_connections() {
    let connections = match crate::system_config::load_connections() {
        Ok(conns) => conns,
        Err(e) => {
            log::warn!("[startup] failed to load connections for table check: {}", e);
            return;
        }
    };

    for conn in &connections {
        if !conn.enabled {
            continue;
        }
        if matches!(
            conn.kind,
            storage_handler::ConnectionKind::Mysql(_) | storage_handler::ConnectionKind::Sqlite(_)
        ) {
            if let Err(e) = storage_handler::ensure_tables_for_connection(&conn.kind).await {
                log::warn!(
                    "[startup] table creation failed for connection '{}' (id={}): {}",
                    conn.name,
                    conn.id,
                    e
                );
            }
        }
    }
}

async fn register_existing_relational_db_pools(state: &api::state::AppState) {
    let connections = match crate::system_config::load_connections() {
        Ok(conns) => conns,
        Err(err) => {
            log::warn!("[startup] failed to load connections for DB pool setup: {}", err);
            return;
        }
    };

    for connection in connections.into_iter().filter(|item| item.enabled) {
        if !matches!(
            connection.kind,
            storage_handler::ConnectionKind::Mysql(_) | storage_handler::ConnectionKind::Sqlite(_)
        ) {
            continue;
        }

        match storage_handler::build_relational_db_connection_for_kind(&connection.id, &connection.kind).await {
            Ok(pool) => {
                state.tasks.lock().unwrap().register_db_pool(connection.id.clone(), pool);
            }
            Err(err) => {
                log::warn!(
                    "[startup] failed to register relational DB pool for connection '{}' (id={}): {}",
                    connection.name,
                    connection.id,
                    err
                );
            }
        }
    }
}

fn spawn_task_ttl_cleanup(state: std::sync::Arc<api::state::AppState>) {
    let ttl_hours = zihuan_core::system_config::load_section::<zihuan_core::system_config::GlobalSettingsSection>()
        .unwrap_or_default()
        .task_ttl_hours;

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let pools: Vec<zihuan_core::data_refs::RelationalDbConnection> =
                state.tasks.lock().unwrap().all_db_pools().into_values().collect();
            for pool in pools {
                if let Err(err) = api::task_store::cleanup_expired_tasks(&pool, ttl_hours).await {
                    log::warn!("Task TTL cleanup failed: {}", err);
                }
            }
        }
    });
}
