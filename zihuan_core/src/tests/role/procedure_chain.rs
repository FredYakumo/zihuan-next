use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::Result;
use crate::model_inference::llm::StreamToken;
use crate::role::procedure::{
    execute_procedure_chain, Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution,
    ProcedureOutput,
};
use crate::role::transport::TransportSink;

/// Transport sink that reports whether it is still referenced, standing in for the turn's
/// token/event channel pair whose closure tells the adapter the turn is finished.
struct TrackingSink {
    alive: Arc<AtomicBool>,
}

impl Drop for TrackingSink {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

impl TransportSink for TrackingSink {
    fn token_sender(&self) -> UnboundedSender<StreamToken> {
        let (token_tx, _token_rx) = tokio::sync::mpsc::unbounded_channel();
        token_tx
    }

    fn send_event(&self, _event: serde_json::Value) {}
}

/// Background procedure parked long enough that the chain has surely returned.
struct ParkedBackground;

#[async_trait]
impl Procedure for ParkedBackground {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "parked_background",
            name: "parked background",
            execution: ProcedureExecution::Background,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(ProcedureOutput::none())
    }
}

fn context_with_sink(alive: &Arc<AtomicBool>) -> ProcedureContext {
    let sink: Arc<dyn TransportSink> = Arc::new(TrackingSink { alive: Arc::clone(alive) });
    ProcedureContext {
        session_id: "session".to_string(),
        is_new_conversation: true,
        latest_user_text: Some("hello".to_string()),
        workspace_path: None,
        transport_out: Some(sink),
        procedure_outputs: Vec::new(),
        role_context: None,
    }
}

/// A detached background procedure must not keep the turn's transport alive.
///
/// The dashboard transport treats the closure of the turn's token and event channels as
/// "this turn is done" and finishing the SSE response behind it. A background procedure that
/// held a sink would therefore hold the response open until its own work finished, delaying
/// every event the turn emits afterwards — including an `ask_user` request raised once
/// inference has already stopped.
#[test]
fn background_procedure_does_not_hold_the_turn_transport() {
    let runtime = tokio::runtime::Runtime::new().expect("test runtime");
    let alive = Arc::new(AtomicBool::new(true));
    let mut context = context_with_sink(&alive);
    let procedures: Vec<Arc<dyn Procedure>> = vec![Arc::new(ParkedBackground)];

    runtime.block_on(async {
        execute_procedure_chain(procedures, &mut context)
            .await
            .expect("chain should not fail");
        // The chain's own context is the caller's, like the turn's; it drops with the turn.
        drop(context);
        // Let the spawned task reach its first await before asserting.
        tokio::task::yield_now().await;
    });

    assert!(
        !alive.load(Ordering::SeqCst),
        "the turn's transport must be released while the background procedure still runs"
    );
}
