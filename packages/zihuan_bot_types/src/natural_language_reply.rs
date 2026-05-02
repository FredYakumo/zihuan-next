use crate::message::{AtTargetMessage, Message, PlainTextMessage};
use serde::Deserialize;
use zihuan_core::error::{Error, Result};

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum QQMessageJsonItem {
    PlainText {
        content: String,
    },
    CombineText {
        content_list: Vec<QQMessageJsonContentItem>,
    },
    At {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum QQMessageJsonContentItem {
    PlainText { content: String },
    At { target: String },
}

pub fn qq_message_json_output_system_prompt() -> &'static str {
    concat!(
        "请你输出发送QQ消息的字符串，你必须只输出纯 JSON 数组，不能输出 markdown、代码块、解释、前后缀文本。\n",
        "你发送的二维数组代表了 QQ 中发送多条消息：外层数组表示总共要发送的多条消息，内层数组表示其中某一次发送的消息内容。\n",
        "顶层必须是二维 JSON 数组：外层数组的每个元素代表一次发送，内层数组代表这次发送里的消息段列表。\n",
        "内层数组元素只支持三种 message_type：\n",
        "1. plain_text: {\"message_type\":\"plain_text\",\"content\":\"文本\"}\n",
        "2. at: {\"message_type\":\"at\",\"target\":\"QQ号\"}，仅在群聊中使用，一般用来提到一个人\n",
        "3. combine_text: {\"message_type\":\"combine_text\",\"content_list\":[上面允许的 at/plain_text 对象列表]}\n",
        "示例：[[{\"message_type\":\"plain_text\",\"content\":\"第一条\"}],[{\"message_type\":\"at\",\"target\":\"123\"},{\"message_type\":\"plain_text\",\"content\":\"第二条\"}]]\n",
        "规则：\n",
        "- 顶层必须输出非空二维 JSON 数组。\n",
        "- 每个内层数组都必须是非空数组。\n",
        "- plain_text.content 必须是非空且不能只有空白。\n",
        "- at.target 必须是非空字符串。\n",
        "- combine_text 表示这些消息段要在同一次发送里组合发送；内容只允许 at/plain_text，且不要在 combine_text 里嵌套 combine_text。\n",
        "- combine_text.content_list 必须是非空数组。\n",
        "- combine_text 里至少包含一个带实际正文的 plain_text；允许出现 plain_text(\" \") 这样的空格段，但不能整条都没有正文。\n",
        "- 除 JSON 数组外不要输出任何其他内容。"
    )
}

pub fn json_to_qq_message_vec(content: &str) -> Result<Vec<Vec<Message>>> {
    let value: serde_json::Value = serde_json::from_str(content)?;
    json_value_to_qq_message_vec(&value)
}

pub fn json_value_to_qq_message_vec(value: &serde_json::Value) -> Result<Vec<Vec<Message>>> {
    let batches: Vec<Vec<QQMessageJsonItem>> = serde_json::from_value(value.clone())?;
    if batches.is_empty() {
        return Err(Error::ValidationError(
            "QQ message JSON array must not be empty".to_string(),
        ));
    }

    batches
        .into_iter()
        .enumerate()
        .map(|(batch_index, items)| {
            if items.is_empty() {
                return Err(Error::ValidationError(format!(
                    "QQ message batch {} must not be empty",
                    batch_index + 1
                )));
            }

            let mut messages = Vec::new();
            for item in items {
                append_item(&mut messages, item)?;
            }
            Ok(messages)
        })
        .collect()
}

fn append_item(messages: &mut Vec<Message>, item: QQMessageJsonItem) -> Result<()> {
    match item {
        QQMessageJsonItem::PlainText { content } => {
            if content.trim().is_empty() {
                return Err(Error::ValidationError(
                    "plain_text.content must not be blank".to_string(),
                ));
            }

            messages.push(Message::PlainText(PlainTextMessage { text: content }));
        }
        QQMessageJsonItem::At { target } => {
            let target = target.trim().to_string();
            if target.is_empty() {
                return Err(Error::ValidationError(
                    "at.target must not be empty".to_string(),
                ));
            }

            messages.push(Message::At(AtTargetMessage {
                target: Some(target),
            }));
        }
        QQMessageJsonItem::CombineText { content_list } => {
            if content_list.is_empty() {
                return Err(Error::ValidationError(
                    "combine_text.content_list must not be empty".to_string(),
                ));
            }

            let mut contains_substantive_text = false;

            for content_item in content_list {
                match content_item {
                    QQMessageJsonContentItem::PlainText { content } => {
                        if content.is_empty() {
                            return Err(Error::ValidationError(
                                "combine_text plain_text.content must not be empty".to_string(),
                            ));
                        }

                        contains_substantive_text |= !content.trim().is_empty();
                        messages.push(Message::PlainText(PlainTextMessage { text: content }));
                    }
                    QQMessageJsonContentItem::At { target } => {
                        let target = target.trim().to_string();
                        if target.is_empty() {
                            return Err(Error::ValidationError(
                                "combine_text at.target must not be empty".to_string(),
                            ));
                        }

                        messages.push(Message::At(AtTargetMessage {
                            target: Some(target),
                        }));
                    }
                }
            }

            if !contains_substantive_text {
                return Err(Error::ValidationError(
                    "combine_text must contain at least one substantive plain_text item"
                        .to_string(),
                ));
            }
        }
    }

    Ok(())
}
