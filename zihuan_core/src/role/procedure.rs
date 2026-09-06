use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;

/// RoleService lifecycle phase at which a procedure runs.
///
/// The conceptual order is fixed: `Transport -> BeforeBrain procedures -> BrainAgent ->
/// AfterBrain procedures -> Transport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedurePhase {
    BeforeBrain,
    AfterBrain,
}

/// How the procedure runner executes a procedure.
///
/// `Blocking` procedures run inside the calling pipeline and their output is returned to the
/// caller (e.g. QQ reply review decides the final message). `Background` procedures are spawned
/// as detached tasks whose results only become visible through their own side effects (e.g.
/// session title sidecar files); they never delay the pipeline.
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
    pub phase: ProcedurePhase,
    pub execution: ProcedureExecution,
}

/// Shared context handed to every procedure of one chat work unit.
///
/// **Design:** Carries the chat-pipeline facts every RoleService agrees on. Data that only makes
/// sense for one RoleService type is passed through `role_context` with type erasure (mirroring
/// the Role Context erasure described in role-service.md); procedures downcast it to their own
/// role-specific context type via [`ProcedureContext::role_context`]. Procedures that need the
/// `RoleServiceConfig` receive it where their procedure set is collected, next to the context.
#[derive(Clone)]
pub struct ProcedureContext {
    pub session_id: String,
    pub is_new_conversation: bool,
    pub latest_user_text: Option<String>,
    pub workspace_path: Option<String>,
    pub role_context: Option<Arc<dyn Any + Send + Sync>>,
}

impl ProcedureContext {
    pub fn role_context<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.role_context.as_ref()?.downcast_ref::<T>()
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

/// A RoleService processing unit that runs outside the main BrainAgent inference.
///
/// Extra processing and side effects of a RoleService turn (context preparation, reply review,
/// conversation naming, ...) are implemented as procedures and executed through
/// [`execute_procedures`] at the fixed lifecycle phase matching their descriptor.
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

/// Execute the given procedures for one lifecycle phase, in order.
///
/// **Design:** `Blocking` procedures are awaited sequentially and a failure short-circuits the
/// whole call so callers can propagate it like a direct call would. `Background` procedures are
/// spawned onto the tokio runtime as detached tasks: failures are logged and never affect the
/// pipeline. Only `Blocking` outputs are returned, in execution order.
///
/// Procedures may carry borrowed data (QQ turn contexts) but then cannot be `Background` —
/// spawning requires `'static`. Use [`execute_blocking_procedures`] for borrowed-only sets.
pub async fn execute_procedures(
    procedures: Vec<Arc<dyn Procedure>>,
    context: &ProcedureContext,
) -> Result<Vec<ProcedureOutput>> {
    let mut outputs = Vec::new();
    for procedure in procedures {
        match procedure.descriptor().execution {
            ProcedureExecution::Blocking => {
                outputs.push(procedure.execute(context).await?);
            }
            ProcedureExecution::Background => {
                let procedure = Arc::clone(&procedure);
                let context = context.clone();
                tokio::spawn(async move {
                    if let Err(err) = procedure.execute(&context).await {
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

/// Execute borrowed `Blocking` procedures that cannot be spawned (they hold references into the
/// caller's turn state). Sequential like [`execute_procedures`]; errors short-circuit.
pub async fn execute_blocking_procedures<'a>(
    procedures: Vec<Arc<dyn Procedure + 'a>>,
    context: &ProcedureContext,
) -> Result<Vec<ProcedureOutput>> {
    let mut outputs = Vec::new();
    for procedure in procedures {
        outputs.push(procedure.execute(context).await?);
    }
    Ok(outputs)
}
