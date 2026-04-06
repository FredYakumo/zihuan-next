mod api;
mod error;
mod init_registry;
mod log_forwarder;
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
#[command(author, version, about = "Zihuan Next — node-graph workflow engine (web UI)")]
struct Args {
    /// Host address to listen on
    #[arg(long, default_value = "127.0.0.1", env = "ZIHUAN_HOST")]
    host: String,

    /// TCP port to listen on
    #[arg(long, default_value_t = 8080, env = "ZIHUAN_PORT")]
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
    log_forwarder::set_broadcast(broadcast.clone());

    let listen_addr = format!("{}:{}", args.host, args.port);
    info!("Starting web server on http://{}", listen_addr);
    info!("Open your browser at  http://{}", listen_addr);

    let router = api::build_router(Arc::clone(&state), broadcast);
    let service = salvo::Service::new(router);
    let acceptor = salvo::conn::TcpListener::new(&listen_addr)
        .try_bind()
        .await
        .expect("Failed to bind TCP listener");
    salvo::Server::new(acceptor).serve(service).await;
}
