//! QQ channel side of the slash-command engine.
//!
//! The command core lives in `zihuan_core::command`: data-driven [`CommandSpec`]s
//! (registered by the service) are executed by a channel-neutral engine over a
//! serializable `Step`/`Effect`/snapshot model. This module is the QQ channel's
//! half of that engine:
//!
//! - [`runtime`] implements `zihuan_core::command::CommandRuntime` for QQ
//!   (`QqChatCommandRuntime`): it resolves `ims://*` step ops, renders effects
//!   into QQ output, and persists pause snapshots.
//! - [`pipeline`] owns the turn-level dispatch: deciding whether an inbound
//!   message is a command, running it through the engine, and classifying the
//!   turn end (consumed / passthrough to the brain / not a command).
//!
//! `claimed.rs` (the brain loop) calls [`pipeline::run_command_pipeline`] and
//! never touches the engine or runtime directly.

mod pipeline;
mod runtime;

pub(crate) use pipeline::{run_command_pipeline, CommandTurnEnd};
pub(crate) use runtime::run_command_effects_now;
