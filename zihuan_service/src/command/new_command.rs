use zihuan_core::command::{
    CommandContext, CommandHandler, CommandResult, CommandSideEffect, NewConversationRequest,
    SideEffectContext,
};
use zihuan_core::error::Result;

// NewCommand — `/new`, `/clear`, `/reset` handler.
//
// ## Purpose
//
// Starts a fresh conversation context for the current channel.
//
// Supported across QQ Chat, OpenAI-compatible HTTP Stream, and the dashboard
// internal chat stream.
//
// ## Design
//
// - Emits a semantic `start_new_conversation` side effect instead of encoding
//   storage-specific keys inside the command layer.
// - The handler itself does **not** touch storage or session state — each
//   runtime path (QQ / HTTP / dashboard) decides what “new conversation” means.

struct StartNewConversationSideEffect {
    request: NewConversationRequest,
}

impl CommandSideEffect for StartNewConversationSideEffect {
    fn execute(&self, ctx: &dyn SideEffectContext) -> Result<()> {
        ctx.start_new_conversation(&self.request)
    }

    fn name(&self) -> &str {
        "start_new_conversation"
    }
}

pub struct NewCommand;

impl CommandHandler for NewCommand {
    fn handle(&self, ctx: &CommandContext, _args: &[String]) -> CommandResult {
        CommandResult {
            reply: "对话历史已清除，开始新的对话。".to_string(),
            side_effects: vec![Box::new(StartNewConversationSideEffect {
                request: NewConversationRequest {
                    caller_id: ctx.caller_id.clone(),
                    channel: ctx.channel.clone(),
                },
            })],
            echo_message: Some("已清空之前对话内容".to_string()),
            inject_to_llm: false,
        }
    }
}
