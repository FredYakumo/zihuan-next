use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;

use super::transport::TransportSink;
use crate::error::Result;

/// How the chain executor runs a procedure.
///
/// `Blocking` procedures run inside the calling pipeline, in the chain's `Vec` order, and their
/// output is returned to the caller and accumulated into [`ProcedureContext::procedure_outputs`]
/// for the following procedures (e.g. QQ reply review reads the brain output). `Background`
/// procedures are spawned as detached tasks whose results only become visible through their own
/// side effects (e.g. session title sidecar files); they never delay the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureExecution {
    Blocking,
    Background,
}

/// Stable metadata describing a procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub execution: ProcedureExecution,
}

/// Shared context handed to every procedure of one chat work unit.
///
/// **Design:** Carries the chat-pipeline facts every RoleService agrees on. Data that only makes
/// sense for one RoleService type is passed through `role_context` with type erasure (mirroring
/// the Role Context erasure described in role-service.md); procedures downcast it to their own
/// role-specific context type via [`ProcedureContext::role_context`]. Procedures that need the
/// `RoleServiceConfig` receive it where their procedure list is assembled, next to the context.
#[derive(Clone)]
pub struct ProcedureContext {
    pub session_id: String,
    pub is_new_conversation: bool,
    pub latest_user_text: Option<String>,
    pub workspace_path: Option<String>,
    /// Transport out boundary of the turn (documents/transport.md). Procedures emit streamed
    /// tokens and turn events through it without knowing the concrete transport.
    pub transport_out: Option<Arc<dyn TransportSink>>,
    /// Outputs of the chain's `Blocking` procedures that already ran, in execution order. The
    /// chain executor appends each output here, so a procedure can consume the outputs of its
    /// predecessors (e.g. the QQ reply review reads the brain output).
    pub procedure_outputs: Vec<ProcedureOutput>,
    pub role_context: Option<Arc<dyn Any + Send + Sync>>,
}

impl ProcedureContext {
    pub fn role_context<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.role_context.as_ref()?.downcast_ref::<T>()
    }

    /// The most recent output of the chain's procedures carrying the given payload type.
    pub fn find_output<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.procedure_outputs.iter().rev().find_map(|output| output.get::<T>())
    }
}

/// Type-erased output of a blocking procedure. Background procedures return [`ProcedureOutput::none`].
#[derive(Clone)]
pub struct ProcedureOutput(Arc<dyn Any + Send + Sync>);

impl ProcedureOutput {
    pub fn of<T: Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(value))
    }

    pub fn none() -> Self {
        Self(Arc::new(()))
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        Arc::clone(&self.0).downcast::<T>().ok()
    }

    /// Clone the stored value out of the output, for convenience at call sites.
    pub fn cloned<T: Send + Sync + Clone + 'static>(&self) -> Option<T> {
        self.get::<T>().map(|value| (*value).clone())
    }
}

/// One processing unit of a RoleService turn.
///
/// Everything a turn does between `Transport in` and `Transport out` — context preparation,
/// the brain invocation, reply review, conversation naming, ... — is implemented as a
/// procedure, and a turn runs a plain `Vec` of them (documents/procedure.md): the execution
/// order is the `Vec` order.
///
/// **The trait anchors the execution flow:** implementors only provide [`Procedure::run`]; every
/// procedure is started through [`Procedure::execute`], which wraps `run` with the uniform
/// logging contract. Executors call `execute` — never `run` directly.
#[async_trait]
pub trait Procedure: Send + Sync {
    fn descriptor(&self) -> ProcedureDescriptor;

    /// The procedure-specific work. Called only by the anchored [`Procedure::execute`] flow.
    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput>;

    /// Anchored execution entry point shared by every procedure: uniform start/finish/failure
    /// logging around the procedure-specific [`Procedure::run`].
    async fn execute(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let descriptor = self.descriptor();
        log::debug!("procedure '{}' started", descriptor.id);
        match self.run(context).await {
            Ok(output) => {
                log::debug!("procedure '{}' finished", descriptor.id);
                Ok(output)
            }
            Err(err) => {
                log::warn!("procedure '{}' failed: {err}", descriptor.id);
                Err(err)
            }
        }
    }
}

/// Execute the given procedure chain in order.
///
/// **Design:** `Blocking` procedures are awaited sequentially, each output is appended to
/// [`ProcedureContext::procedure_outputs`] before the next procedure runs, and a failure
/// short-circuits the chain so callers can propagate it like a direct call would. `Background`
/// procedures are spawned onto the tokio runtime as detached tasks: failures are logged and
/// never affect the chain. Only `Blocking` outputs are returned, in execution order.
///
/// Procedures may carry borrowed data (QQ turn contexts) but then cannot be `Background` —
/// spawning requires `'static`. Use [`execute_blocking_procedure_chain`] for borrowed-only
/// chains.
pub async fn execute_procedure_chain(
    procedures: Vec<Arc<dyn Procedure>>,
    context: &mut ProcedureContext,
) -> Result<Vec<ProcedureOutput>> {
    let mut outputs = Vec::new();
    for procedure in procedures {
        match procedure.descriptor().execution {
            ProcedureExecution::Blocking => {
                let output = procedure.execute(context).await?;
                context.procedure_outputs.push(output.clone());
                outputs.push(output);
            }
            ProcedureExecution::Background => {
                let procedure = Arc::clone(&procedure);
                let background_context = context.clone();
                tokio::spawn(async move {
                    if let Err(err) = procedure.execute(&background_context).await {
                        log::warn!(
                            "background procedure '{}' dropped: {err}",
                            procedure.descriptor().id
                        );
                    }
                });
            }
        }
    }
    Ok(outputs)
}

/// Execute a borrowed procedure chain that cannot be spawned (its procedures hold references
/// into the caller's turn state). Every procedure runs sequentially as `Blocking`; errors
/// short-circuit. Otherwise identical to [`execute_procedure_chain`].
pub async fn execute_blocking_procedure_chain<'a>(
    procedures: Vec<Arc<dyn Procedure + 'a>>,
    context: &mut ProcedureContext,
) -> Result<Vec<ProcedureOutput>> {
    let mut outputs = Vec::new();
    for procedure in procedures {
        let output = procedure.execute(context).await?;
        context.procedure_outputs.push(output.clone());
        outputs.push(output);
    }
    Ok(outputs)
}
