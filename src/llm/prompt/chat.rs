use crate::{bot_adapter::{adapter::BotAdapter, models::MessageEvent}, llm::{Message, SystemMessage}};

/// Build system message for chat agent based on bot profile and event context
pub fn build_chat_system_message(bot_adapter: &BotAdapter, event: &MessageEvent, persona: &str) -> Message {
    let bot_profile = bot_adapter.get_bot_profile();
    
    if let Some(profile) = bot_profile {
        if event.is_group_message {
            SystemMessage(format!(
                "你是\"{}\"（QQ号: {}）。在群\"{}\"中，用户\"{}\"（QQ号: {}）向你发送了消息。\n\
                你需要以{}的性格生成对话回复。",
                profile.nickname,
                profile.qq_id,
                event.group_name.clone().unwrap_or_default(),
                if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                event.sender.user_id,
                persona
            ))
        } else {
            SystemMessage(format!(
                "你是\"{}\"（QQ号: {}）。你的好友\"{}\"（QQ号: {}）向你发送了消息。\n\
                你需要以{}的性格生成对话回复。",
                profile.nickname,
                profile.qq_id,
                event.sender.nickname,
                event.sender.user_id,
                persona
            ))
        }
    } else {
        SystemMessage(format!(
            "你是\"紫幻\"（QQ号: {}）。你的职责是进行自然对话。\n\
            你需要以{}的性格生成对话回复。",
            bot_adapter.get_bot_id(),
            persona
        ))
    }
}