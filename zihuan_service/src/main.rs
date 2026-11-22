mod api;
mod error;
mod init_registry;
mod log_forwarder;
mod system_config;
mod util;

use std::sync::Arc;

use clap::Parser;
use lazy_static::lazy_static;
use log::{error, info};
use log_util::log_util::LogUtil;
use salvo::Listener;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next", "logs");
}

#[derive(Parser, Debug)]
#[command(
    name = "zihuan_next",
    author,
    version = env!("ZIHUAN_BUILD_VERSION"),
    about = "Zihuan Next — node-graph workflow engine (web UI)"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1", env = "ZIHUAN_HOST")]
    host: String,

    #[arg(long, default_value_t = 9951, env = "ZIHUAN_PORT")]
    port: u16,
}

#[tokio::main]
async fn main() {
    // Parse before anything else so `--version` / `--help` answer immediately and
    // without side effects: the release scripts read the version from this flag, and
    // `build.rs` is the single place that decides what the version string is.
    let args = Args::parse();

    log_forwarder::init(&BASE_LOG);
    log_forwarder::install_panic_hook();

    if let Err(e) = init_registry::init_node_registry() {
        error!("Failed to initialize node registry: {}", e);
    } else {
        info!("Node registry initialized");
    }

    match init_registry::start_node_worker().await {
        Ok(()) => info!("Dynamic Script Runtime started"),
        Err(error) => error!("Failed to start Dynamic Script Runtime: {error}"),
    }

    // Initialize global command registry.
    {
        let registry = zihuan_service::command::init_global_command_registry();
        info!("Command registry initialized with {} commands", registry.list_commands().len());
    }

    zihuan_core::ims_bot_adapter::runtime::wechat_pad::set_service_port(args.port);

    match tokio::task::spawn_blocking(zihuan_core::system_database::initialize).await {
        Ok(Ok(())) => info!("System configuration database initialized"),
        result => {
            error!("Failed to initialize system configuration database: {result:?}");
            std::process::exit(1);
        }
    }

    if let Err(error) = zihuan_core::agent::declarative_agent::seed_builtin_agents() {
        error!("Failed to seed default agent definitions: {error}");
    }

    if let Err(error) = zihuan_core::scheduler::init_script_jobs() {
        error!("Failed to initialize scheduled jobs: {error}");
    }
    zihuan_core::scheduler::start_scheduler_kernel();

    let state = Arc::new(api::state::AppState::new());
    let broadcast = api::ws::create_broadcast();
    log_forwarder::set_app_state(Arc::clone(&state));
    log_forwarder::set_broadcast(broadcast.clone());
    api::ws::install_qq_context_emitter(broadcast.clone());
    // Register the task runtime before the first scheduler tick so scheduled job runs are
    // recorded as tasks even when no role service has been started yet.
    api::config::role_services::build_agent_task_runtime(Arc::clone(&state), broadcast.clone());

    if let Err(error) = api::planer_runtime::initialize(
        state.clone(),
        broadcast.clone(),
        api::config::role_services::build_agent_task_runtime(state.clone(), broadcast.clone()),
    ) {
        error!("Failed to initialize Planer: {error}");
    }

    api::task_store::spawn_ttl_cleanup(Arc::clone(&state));

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

    let model_http_endpoint = format!("http://{display_addr}/v1/chat/completions");
    let router = api::build_router(
        Arc::clone(&state),
        broadcast.clone(),
        canonical_local_origin,
        model_http_endpoint,
    );
    let service = salvo::Service::new(router);
    let acceptor = salvo::conn::TcpListener::new(&listen_addr)
        .try_bind()
        .await
        .expect("Failed to bind TCP listener");
    let startup_state = Arc::clone(&state);
    tokio::spawn(async move {
        let state = startup_state;
        api::task_store::recover_orphan_running_tasks(&state).await;
        api::config::role_services::auto_start_role_services(Arc::clone(&state), broadcast).await;
    });
    salvo::Server::new(acceptor).serve(service).await;
}
