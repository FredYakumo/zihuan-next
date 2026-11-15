//! Task-local scope that tags log records with the role service being restarted.
//!
//! While a restart future runs inside [`scope`], every `log::*` record emitted
//! from that poll (including nested service manager calls) is additionally
//! broadcast as a `ServerMessage::ServiceRuntimeLog`, so the restart dialog can
//! stream exactly the logs of the service it restarts. Records from other tasks
//! polled on the same worker thread are outside the scope and stay untagged.

use tokio::task_local;

task_local! {
    static CURRENT_RESTARTING_SERVICE: String;
}

/// Runs `fut` with every log record tagged by `role_service_id`.
pub async fn scope<F: std::future::Future>(
    role_service_id: impl Into<String>,
    fut: F,
) -> F::Output {
    CURRENT_RESTARTING_SERVICE.scope(role_service_id.into(), fut).await
}

/// The role service id of the restart currently polling, if any.
pub fn current() -> Option<String> {
    CURRENT_RESTARTING_SERVICE.try_with(Clone::clone).ok()
}
