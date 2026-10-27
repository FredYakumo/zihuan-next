//! Shared non-procedure helpers of the procedure layer.
//!
//! These carry no [`Procedure`](zihuan_core::role::procedure::Procedure) implementation of
//! their own; they hold the pure logic that procedures and their callers (send orchestration,
//! tools) reuse.

pub(crate) mod reply_plan;
