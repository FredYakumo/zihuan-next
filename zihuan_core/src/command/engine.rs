use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::effect::Effect;
use super::snapshot::{CmdState, ExecutionSnapshot, InputGate, Invocation, Phase};
use super::{CommandSpec, Step};

/// Default total step budget guarding against runaway expansion / recursion.
pub const MAX_COMMAND_STEPS: usize = 128;

/// The channel-side executor. Each role-service channel constructs one instance
/// per turn. Every value crossing the trait boundary is serializable; only the
/// implementation itself touches channel state (session, senders, store).
pub trait CommandRuntime {
    /// Execute one step. Emit zero or more effects and return how the engine
    /// should continue.
    fn run_step(
        &mut self,
        step: &Step,
        inv: &Invocation,
        state: &mut CmdState,
    ) -> Result<StepRun>;

    /// Render one effect to the channel output.
    fn apply_effect(&mut self, effect: &Effect) -> Result<()>;

    /// Persist a paused execution (an input gate). Channels that support
    /// cross-message resumption store the snapshot here; others default to a
    /// no-op (the pause still ends the turn with the emitted effects).
    fn persist_pause(&mut self, _snapshot: &ExecutionSnapshot) -> Result<()> {
        Ok(())
    }
}

/// Result of executing one step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRun {
    #[serde(default)]
    pub effects: Vec<Effect>,
    pub control: StepControl,
}

impl StepRun {
    pub fn continue_with(effects: Vec<Effect>) -> Self {
        Self { effects, control: StepControl::Continue }
    }

    pub fn emit(effect: Effect) -> Self {
        Self { effects: vec![effect], control: StepControl::Continue }
    }

    pub fn stop_with(effects: Vec<Effect>) -> Self {
        Self { effects, control: StepControl::Stop }
    }

    pub fn expand(steps: Vec<Step>) -> Self {
        Self { effects: Vec::new(), control: StepControl::Expand(steps) }
    }

    pub fn require_input(gate: InputGate) -> Self {
        Self { effects: Vec::new(), control: StepControl::RequireInput(gate) }
    }

    pub fn resume_command(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            effects: Vec::new(),
            control: StepControl::Resume(ResumeAction::Command {
                command: command.into(),
                args,
                ctx_override: None,
                initial_state: CmdState::default(),
            }),
        }
    }

    pub fn resume_command_with_ctx(
        command: impl Into<String>,
        args: Vec<String>,
        ctx_override: super::CommandContext,
        initial_state: CmdState,
    ) -> Self {
        Self {
            effects: Vec::new(),
            control: StepControl::Resume(ResumeAction::Command {
                command: command.into(),
                args,
                ctx_override: Some(ctx_override),
                initial_state,
            }),
        }
    }

    pub fn resume_command_with_state(
        command: impl Into<String>,
        args: Vec<String>,
        initial_state: CmdState,
    ) -> Self {
        Self {
            effects: Vec::new(),
            control: StepControl::Resume(ResumeAction::Command {
                command: command.into(),
                args,
                ctx_override: None,
                initial_state,
            }),
        }
    }

    pub fn resume_snapshot(snapshot: ExecutionSnapshot) -> Self {
        Self { effects: Vec::new(), control: StepControl::Resume(ResumeAction::Snapshot(snapshot)) }
    }
}

/// Control-flow decision produced by a step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StepControl {
    /// Advance to the next step of the current phase.
    Continue,
    /// Inject sub-steps to run before continuing (bounded by the step budget).
    Expand(Vec<Step>),
    /// Pause until the given input arrives.
    RequireInput(InputGate),
    /// Switch into another command run inside the same turn (e.g. /auth resumes
    /// the command that requested authorization).
    Resume(ResumeAction),
    /// End the command, consuming the turn (unless a passthrough is present).
    Stop,
}

/// Target of a same-turn switch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResumeAction {
    /// Fresh resumed run of another spec (skips setup, starts at conditions).
    /// `ctx_override` replaces the invocation context when the original request
    /// context differs from the current message (legacy privilege rows);
    /// `initial_state` seeds the shared state when the paused record carries no
    /// persisted snapshot.
    Command {
        command: String,
        args: Vec<String>,
        ctx_override: Option<super::CommandContext>,
        initial_state: CmdState,
    },
    /// Continue a previously persisted snapshot.
    Snapshot(ExecutionSnapshot),
}

/// Terminal output of one engine run (fresh, resumed or paused).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionResult {
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub passthrough: Option<String>,
    /// Whether a StartNewConversation effect was queued.
    #[serde(default)]
    pub new_session: bool,
    /// Set when the run paused waiting for external input.
    #[serde(default)]
    pub waiting: Option<ExecutionSnapshot>,
}

impl ExecutionResult {
    pub fn finish(mut effects: Vec<Effect>, passthrough: Option<String>) -> Self {
        let new_session = effects.iter().any(|e| matches!(e, Effect::StartNewConversation));
        effects.shrink_to_fit();
        Self { effects, passthrough, new_session, waiting: None }
    }

    pub fn paused(
        effects: Vec<Effect>,
        passthrough: Option<String>,
        snapshot: ExecutionSnapshot,
    ) -> Self {
        Self { effects, passthrough, new_session: false, waiting: Some(snapshot) }
    }
}

/// Resolve a spec by primary name during resume switches.
type SpecResolver<'a> = dyn Fn(&str) -> Option<Arc<CommandSpec>> + 'a;

struct RunState {
    spec: Arc<CommandSpec>,
    phase: Phase,
    idx: usize,
    expanded: Vec<Step>,
    state: CmdState,
    invocation: Invocation,
}

/// Run a command spec fresh (or as a same-turn resume) against a channel
/// runtime. The invocation carries parsed args and passthrough; the engine
/// resolves same-turn resume targets through the global registry.
pub fn execute_command(
    runtime: &mut dyn CommandRuntime,
    spec: Arc<CommandSpec>,
    invocation: Invocation,
) -> Result<ExecutionResult> {
    let resolver = |name: &str| super::resolve_spec(name);
    run_machine(runtime, &resolver, start_run(spec, invocation), Vec::new())
}

/// Resume a persisted snapshot. Setup is skipped (the resumed invocation flag
/// is set) and execution continues from the stored phase/cursor with the stored
/// shared state. Pending effects carried by the snapshot are flushed first.
pub fn resume_from_snapshot(
    runtime: &mut dyn CommandRuntime,
    snapshot: ExecutionSnapshot,
) -> Result<ExecutionResult> {
    let resolver = |name: &str| super::resolve_spec(name);
    let Some(spec) = super::resolve_spec(&snapshot.spec_name) else {
        return Err(crate::validation_error!(
            "resume snapshot references unknown command '/{}'",
            snapshot.spec_name
        ));
    };
    let mut invocation = snapshot.invocation;
    invocation.resumed = true;
    run_machine(
        runtime,
        &resolver,
        RunState {
            spec,
            phase: snapshot.phase,
            idx: snapshot.step_index,
            expanded: snapshot.expanded,
            state: snapshot.state,
            invocation,
        },
        snapshot.pending_effects,
    )
}

/// Run the machine until it completes, pauses, or stops.
///
/// `pending` carries effects produced by earlier phases / a previous command in
/// the same turn (e.g. the /auth success announce before a resumed command).
fn run_machine(
    runtime: &mut dyn CommandRuntime,
    resolve: &SpecResolver<'_>,
    mut run: RunState,
    mut pending_effects: Vec<Effect>,
) -> Result<ExecutionResult> {
    let mut budget = MAX_COMMAND_STEPS;
    loop {
        if budget == 0 {
            return Err(crate::validation_error!(
                "command step budget exceeded while executing /{}",
                run.spec.name
            ));
        }
        budget -= 1;

        let (step, from_phase_list) = if let Some(step) = run.expanded.pop() {
            (step, false)
        } else {
            let list = phase_steps(&run.spec, run.phase);
            if run.idx < list.len() {
                let step = list[run.idx].clone();
                run.idx += 1;
                (step, true)
            } else {
                match run.phase.next() {
                    Some(next) => {
                        run.phase = next;
                        run.idx = 0;
                        continue;
                    }
                    None => break,
                }
            }
        };

        let step_run = runtime.run_step(&step, &run.invocation, &mut run.state)?;
        // Apply SetContext effects into the shared state directly; everything
        // else goes to the runtime's effect sink.
        for effect in step_run.effects {
            match effect {
                Effect::SetContext { key, value } => {
                    run.state.insert(key, value);
                }
                other => pending_effects.push(other),
            }
        }

        match step_run.control {
            StepControl::Continue => {}
            StepControl::Stop => {
                return Ok(ExecutionResult::finish(pending_effects, run.invocation.passthrough));
            }
            StepControl::Expand(mut sub) => {
                // Reversed so sub-steps pop in declared order.
                sub.reverse();
                run.expanded.extend(sub);
            }
            StepControl::RequireInput(gate) => {
                // Do not advance the cursor: the guard re-runs after resume.
                if from_phase_list {
                    run.idx -= 1;
                } else {
                    run.expanded.push(step);
                }
                let snapshot = ExecutionSnapshot {
                    version: ExecutionSnapshot::CURRENT_VERSION,
                    spec_name: run.spec.name.clone(),
                    invocation: run.invocation.clone(),
                    phase: run.phase,
                    step_index: run.idx,
                    expanded: run.expanded,
                    state: run.state,
                    pending_effects: Vec::new(),
                    gate,
                };
                runtime.persist_pause(&snapshot)?;
                return Ok(ExecutionResult::paused(
                    pending_effects,
                    run.invocation.passthrough,
                    snapshot,
                ));
            }
            StepControl::Resume(action) => match action {
                ResumeAction::Command { command, args, ctx_override, initial_state } => {
                    let Some(spec) = resolve(&command) else {
                        return Err(crate::validation_error!(
                            "resume target '/{command}' is not registered"
                        ));
                    };
                    let ctx = ctx_override.unwrap_or_else(|| run.invocation.ctx.clone());
                    run = RunState {
                        spec,
                        phase: Phase::Conditions,
                        idx: 0,
                        expanded: Vec::new(),
                        state: initial_state,
                        invocation: Invocation { ctx, args, passthrough: None, resumed: true },
                    };
                }
                ResumeAction::Snapshot(snapshot) => {
                    let Some(spec) = resolve(&snapshot.spec_name) else {
                        return Err(crate::validation_error!(
                            "resume snapshot references unknown command '/{}'",
                            snapshot.spec_name
                        ));
                    };
                    let mut invocation = snapshot.invocation;
                    invocation.resumed = true;
                    run = RunState {
                        spec,
                        phase: snapshot.phase,
                        idx: snapshot.step_index,
                        expanded: snapshot.expanded,
                        state: snapshot.state,
                        invocation,
                    };
                }
            },
        }
    }

    Ok(ExecutionResult::finish(pending_effects, run.invocation.passthrough))
}

fn start_run(
    spec: Arc<CommandSpec>,
    invocation: Invocation,
) -> RunState {
    let phase = if invocation.resumed { Phase::Conditions } else { Phase::Setup };
    RunState {
        spec,
        phase,
        idx: 0,
        expanded: Vec::new(),
        state: CmdState::default(),
        invocation,
    }
}

fn phase_steps(spec: &CommandSpec, phase: Phase) -> &[Step] {
    match phase {
        Phase::Setup => &spec.setup,
        Phase::Conditions => &spec.conditions,
        Phase::Body => &spec.body,
    }
}
