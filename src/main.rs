mod api;
mod error;
mod init_registry;
mod log_forwarder;
mod service;
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
    author,
    version,
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
    log_forwarder::init(&BASE_LOG);

    if let Err(e) = init_registry::init_node_registry() {
        error!("Failed to initialize node registry: {}", e);
    } else {
        info!("Node registry initialized");
    }

    let args = Args::parse();

    let state = Arc::new(api::state::AppState::new());
    let broadcast = api::ws::create_broadcast();
    log_forwarder::set_app_state(Arc::clone(&state));
    log_forwarder::set_broadcast(broadcast.clone());

    // Auto-start enabled agents and register each as a task.
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
            api::config::agents::start_agent_with_task(
                Arc::clone(&state),
                broadcast.clone(),
                agent,
                connections.clone(),
                None,
            )
            .await;
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
