use crate::{
    bot_adapter::{adapter::BotAdapter, models::MessageEvent},
    llm::{Message, SystemMessage},
};

/// Build system message based on bot profile and event context
pub fn build_system_message(bot_adapter: &BotAdapter, event: &MessageEvent, persona: &str) -> Message {
    let bot_profile = bot_adapter.get_bot_profile();

    if let Some(profile) = bot_profile {
        if event.is_group_message {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。群\"{}\"里的一个叫\"{}\"(QQ号: \"{}\")的人给你发送了一条消息。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname,
                profile.qq_id,
                event.group_name.clone().unwrap_or_default(),
                if !event.sender.card.is_empty() { event.sender.card.clone() } else { event.sender.nickname.clone() },
                event.sender.user_id,
                persona
            ))
        } else {
            SystemMessage(format!(
                "你是\"{}\"，QQ号是\"{}\"。你的好友\"{}\"(QQ号: \"{}\")给你发送了一条消息。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成",
                profile.nickname, profile.qq_id, event.sender.nickname, event.sender.user_id, persona
            ))
        }
    } else {
        SystemMessage(format!(
            "你是\"紫幻\", QQ号是\"{}\"。你的性格是: {}, 你需要根据消息内容决定做出反应或者无反应，其中你做出的反应需要委派给相应的Agent智能体(通过function tools)来完成", 
            bot_adapter.get_bot_id(),
            persona
        ))
    }
}
