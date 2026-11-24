pub mod api;
pub mod cli;
pub mod config;
mod docker_config;
mod installation;
mod native;
mod orchestrator;
mod self_install;
mod storage;
pub mod tui;

#[cfg(test)]
mod tests;

use salvo::{conn::Acceptor, Listener, Router};
use std::{path::PathBuf, sync::Arc};

/// Shared local backend for the terminal and desktop applications.
pub struct Session {
    pub url: String,
    pub token: String,
    server: tokio::task::JoinHandle<()>,
    _lock: std::fs::File,
}

impl Session {
    pub async fn start(root: Option<PathBuf>, frontend: Option<Router>) -> Result<Self, String> {
        let root = root
            .unwrap_or_else(|| zihuan_core::system_config::app_data_dir().join("zihuan-composer"));
        let lock = config::initialize(root)?;
        api::components::recover_interrupted_installs()?;
        let state = Arc::new(api::state::AppState::new());
        let acceptor = salvo::conn::TcpListener::new("127.0.0.1:0")
            .try_bind()
            .await
            .map_err(|error| error.to_string())?;
        let address =
            acceptor.holdings()[0].local_addr.clone().into_std().ok_or("无法读取监听地址")?;
        let mut router = api::router(state.clone());
        if let Some(frontend) = frontend {
            router = router.push(frontend);
        }
        let server = tokio::spawn(async move {
            salvo::Server::new(acceptor).serve(router).await;
        });
        Ok(Self {
            url: format!("http://{address}"),
            token: state.token.clone(),
            server,
            _lock: lock,
        })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.server.abort();
    }
}
