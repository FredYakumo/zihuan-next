use zihuan_core::command::{
    execute_command, global_command_registry, CommandChannel, CommandContext, Effect, Invocation,
};
use zihuan_core::error::Result;

use crate::qq_chat::logging::QqChatTaskTrace;
use crate::qq_chat::model::{QqChatAgentServiceContext, QqChatServiceTurnResult};
use crate::role_config::QqChatEmotionDimensionConfig;

use super::runtime::{make_runtime, make_runtime_with_emotion};

/// Result of running the command state machine for one QQ turn.
pub(crate) enum CommandTurnEnd {
    /// The command consumed the whole turn (reply sent, or waiting for auth).
    Consumed(QqChatServiceTurnResult),
    /// The command produced leftover passthrough text that should feed the brain.
    Passthrough(String),
    /// The message is not a command; continue normal handling.
    NotACommand,
}

/// Runs a message through the unified command state machine. Returns the
/// turn end when the message was a command, or `NotACommand` to fall
/// through to the brain loop.
///
/// Delivery semantics preserve the pre-engine QQ behavior:
/// - command effects are rendered in order (Text = direct reply, Forward =
///   forward node, StartNewConversation = clear history);
/// - a pause (auth gate) sends the auth prompt and persists the resume
///   snapshot; the later `/auth` resumes the same spec via the engine.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_command_pipeline(
    agent_id: &str,
    trace: &QqChatTaskTrace,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    raw_user_message: &str,
    sender_id: &str,
    target_id: &str,
    bot_id: &str,
    is_group: bool,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    ctx: &QqChatAgentServiceContext<'_>,
) -> Result<CommandTurnEnd> {
    let Some(registry) = global_command_registry() else {
        return Ok(CommandTurnEnd::NotACommand);
    };
    if !raw_user_message.trim_start().starts_with('/') {
        return Ok(CommandTurnEnd::NotACommand);
    }
    let cmd_ctx = CommandContext {
        agent_type: "qq_chat".to_string(),
        agent_id: agent_id.to_string(),
        caller_id: sender_id.to_string(),
        channel: CommandChannel::QqChat {
            sender_id: sender_id.to_string(),
            is_group,
            group_id: inference_event.group_id,
            target_id: target_id.to_string(),
        },
    };
    let Some((spec, parsed)) = registry.spec_for(&cmd_ctx, raw_user_message) else {
        // Unmatched or scope-mismatched slash line: fall through to the brain.
        return Ok(CommandTurnEnd::NotACommand);
    };
    // Permission gate.
    let permission = registry.check_permission(&cmd_ctx, raw_user_message);
    if permission.matched && !permission.allowed {
        let denied = deliver_command_effects(
            trace,
            ctx,
            event,
            inference_event,
            sender_id,
            target_id,
            bot_id,
            agent_id,
            is_group,
            vec![Effect::Text("你没有权限使用此命令。".to_string())],
        )?;
        debug_assert!(denied, "permission-denied command must emit a reply");
        return Ok(CommandTurnEnd::Consumed(QqChatServiceTurnResult {
            result_summary: "命令权限拒绝".to_string(),
        }));
    }

    let mut runtime = make_runtime_with_emotion(
        trace,
        ctx,
        agent_id,
        event,
        inference_event,
        sender_id,
        target_id,
        bot_id,
        is_group,
        emotion_dimensions,
    );
    let invocation = Invocation {
        ctx: cmd_ctx.clone(),
        args: parsed.args.clone(),
        passthrough: parsed.passthrough_text.clone(),
        resumed: false,
    };
    let result = execute_command(&mut runtime, spec, invocation)?;

    if result.waiting.is_some() {
        // Deliver the gate prompt produced by the paused step, then end the
        // turn waiting for the /auth message. The resume snapshot was
        // persisted by the runtime's persist_pause hook.
        let _ = deliver_command_effects(
            trace,
            ctx,
            event,
            inference_event,
            sender_id,
            target_id,
            bot_id,
            agent_id,
            is_group,
            result.effects.clone(),
        )?;
        return Ok(CommandTurnEnd::Consumed(QqChatServiceTurnResult {
            result_summary: "命令已进入等待授权状态".to_string(),
        }));
    }

    let delivered = deliver_command_effects(
        trace,
        ctx,
        event,
        inference_event,
        sender_id,
        target_id,
        bot_id,
        agent_id,
        is_group,
        result.effects.clone(),
    )?;
    // A command that produced leftover passthrough text (e.g. `/new <text>`)
    // delivers its effects first, then the remainder feeds the brain loop —
    // matching the pre-engine QQ behavior.
    if let Some(passthrough) = result.passthrough {
        return Ok(CommandTurnEnd::Passthrough(passthrough));
    }
    if delivered {
        return Ok(CommandTurnEnd::Consumed(QqChatServiceTurnResult {
            result_summary: "已处理命令".to_string(),
        }));
    }
    // Effect-free command with no passthrough: fall through.
    Ok(CommandTurnEnd::NotACommand)
}

/// Render command effects into QQ output. Returns `true` when any effect
/// was delivered (i.e. the command consumed the turn).
#[allow(clippy::too_many_arguments)]
fn deliver_command_effects(
    trace: &QqChatTaskTrace,
    ctx: &QqChatAgentServiceContext<'_>,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    sender_id: &str,
    target_id: &str,
    bot_id: &str,
    agent_id: &str,
    is_group: bool,
    effects: Vec<Effect>,
) -> Result<bool> {
    if effects.is_empty() {
        return Ok(false);
    }
    let mut runtime = make_runtime(
        trace,
        ctx,
        agent_id,
        event,
        inference_event,
        sender_id,
        target_id,
        bot_id,
        is_group,
    );
    for effect in &effects {
        zihuan_core::command::CommandRuntime::apply_effect(&mut runtime, effect)?;
    }
    Ok(true)
}
