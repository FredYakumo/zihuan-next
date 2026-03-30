use std::collections::HashMap;

use crate::bot_adapter::models::message::{AtTargetMessage, Message, PlainTextMessage};
use crate::error::{Error, Result};
use crate::llm::llm_base::LLMBase;
use crate::llm::{InferenceParam, OpenAIMessage};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use serde::Deserialize;

pub const TARGET_TYPE_FRIEND: &str = "friend";
pub const TARGET_TYPE_GROUP: &str = "group";

pub struct NaturalLanguageReplyNode {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum NaturalReplyItem {
    PlainText {
        content: String,
    },
    CombineText {
        content_list: Vec<NaturalReplyContentItem>,
    },
    At {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum NaturalReplyContentItem {
    PlainText { content: String },
    At { target: String },
}

#[derive(Debug, Clone)]
pub struct QQMessageBatch {
    pub messages: Vec<Message>,
    pub text_length: usize,
}

#[derive(Debug, Clone)]
pub struct QQNaturalLanguageReplyInference {
    pub raw_reply_content: String,
    pub batches: Vec<QQMessageBatch>,
}

impl NaturalLanguageReplyNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

pub fn normalize_target_type(value: Option<&DataValue>) -> &'static str {
    match value {
        Some(DataValue::String(target_type))
            if target_type.eq_ignore_ascii_case(TARGET_TYPE_GROUP) =>
        {
            TARGET_TYPE_GROUP
        }
        _ => TARGET_TYPE_FRIEND,
    }
}

pub fn build_system_prompt(
    character_name: &str,
    style: &str,
    target_id: &str,
    target_type: &str,
    max_one_reply_length: usize,
    mention_target_id: Option<&str>,
) -> String {
    let target_type_label = if target_type == TARGET_TYPE_GROUP {
        "群聊"
    } else {
        "好友私聊"
    };
    let mut prompt = format!(
        concat!(
            "你要扮演 QQ 角色“{character_name}”，把 user 提供的原始话术修改成用 {style} 的语言风格表达、自然真实且可直接发送的 QQ 消息。\n",
            "当前回复目标类型：{target_type_label}；目标 ID：{target_id}。\n",
            "传给你的第一条 user 消息就是待修改的原文草稿。\n",
            "你的任务是基于这段原文做润色、改写或轻微重组，让它更符合指定风格，但本质上仍然是在修改这段话本身。\n",
            "不要把这段原文当成别人刚发来的消息再回复一次；输出应当是这段原文的修改版，而不是针对它的新回复。\n",
            "你必须只输出纯 JSON 数组，不能输出 markdown、代码块、解释、前后缀文本。\n",
            "数组元素支持三种 message_type：\n",
            "1. plain_text: {{\"message_type\":\"plain_text\",\"content\":\"文本\"}}\n",
            "2. at: {{\"message_type\":\"at\",\"target\":\"QQ号\"}}\n",
            "3. combine_text: {{\"message_type\":\"combine_text\",\"content_list\":[上面允许的 at/plain_text 对象列表]}}\n",
            "规则：\n",
            "- plain_text.content 必须是非空字符串，且尽量不超过 {max_one_reply_length} 个字符。\n",
            "- 默认只输出 1 个顶层数组元素；只有当单个 plain_text 或 combine_text 无法在长度限制内完整表达时，才拆成多个顶层数组元素。\n",
            "- 对于简短、日常、无复杂任务的输入，回复要保持简短自然，优先 1 句，必要时最多 2 句，不要主动扩写成长段。\n",
            "- 如果 user 消息本身就是大段正文、代码、教程步骤、配置片段或较长说明，你的首要目标是保留内容完整性，不要擅自总结、压缩、删节成短回复。\n",
            "- 当原文较长而单条消息放不下时，应按原有顺序拆成多个顶层数组元素连续输出，宁可分多条发送，也不要只保留前面一部分。\n",
            "- 对于代码块、命令、配置、编号步骤、换行排版，尽量保留原有结构与信息；可以润色前后说明语气，但不要破坏代码和关键内容。\n",
            "- 不要为了停顿、强调或节奏感，把同一段连续回复拆成多个 plain_text 或多个顶层数组元素。\n",
            "- 如果 user 消息已经接近可直接发送，请优先轻微润色并保留原意，不要改写成完全不同的话。\n",
            "- 优先保留原句中的核心信息、称呼、语气方向和说话人身份，例如“我是紫幻，你的朋友”这类自我介绍通常应保留而不是删掉。\n",
            "- 不要凭空补充 user 消息里没有出现的新事实、新理由、新事件或新上下文。\n",
            "- combine_text 表示这些消息段要在一次发送里组合发送；不要在 combine_text 里嵌套 combine_text。\n",
            "- 如果需要形成更真实的“@某人 文本”效果，请使用 combine_text，并把 at 放在文本前面。\n",
            "- 顶层 at 只允许用于群聊场景。\n",
            "- combine_text 里至少包含一个带实际正文的 plain_text，避免只发纯 @ 或只有空格。\n",
            "- 如果你要输出“@某人 文本”且文本较长，请自行拆成多个 combine_text，而不是输出一个超长 combine_text。\n",
            "- 除 JSON 数组外不要输出任何其他内容。"
        ),
        character_name = character_name,
        style = style,
        target_type_label = target_type_label,
        target_id = target_id,
        max_one_reply_length = max_one_reply_length,
    );

    if target_type == TARGET_TYPE_GROUP {
        if let Some(mention_target_id) = mention_target_id
            .map(str::trim)
            .filter(|mention_target_id| !mention_target_id.is_empty())
        {
            prompt.push_str(&format!(
                concat!(
                    "\n群聊定向回复补充规则：\n",
                    "- 当前发送目标群号是 {target_id}，本次优先回复给群成员 {mention_target_id}。\n",
                    "- 默认优先使用 combine_text 表达“@对方 空格 正文”。\n",
                    "- 如果要 @ 该成员，content_list 优先按 at({mention_target_id})、plain_text(\" \")、plain_text(\"正文\") 的顺序组织。\n",
                    "- @ 后面必须保留一个半角空格，不要紧贴正文，也不要省略 @。\n",
                    "- 如果正文过长需要拆分，首条优先包含上面的 @ + 空格，后续条目再延续正文。"
                ),
                target_id = target_id,
                mention_target_id = mention_target_id,
            ));
        }
    }

    prompt
}

pub fn split_plain_text(content: &str, max_one_reply_length: usize) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    if content.chars().count() <= max_one_reply_length {
        return vec![content.to_string()];
    }

    let lines = split_by_newline(content, max_one_reply_length);
    if lines.len() > 1 {
        return lines;
    }

    split_by_delimiters(content, max_one_reply_length, &['。', '！', '？', '；', '\n'])
}

fn split_by_newline(content: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;

    for line in content.split_inclusive('\n') {
        let line_len = line.chars().count();
        if line_len > max_len {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
                current_len = 0;
            }
            chunks.extend(split_by_delimiters(
                line,
                max_len,
                &['\n', '。', '！', '？', '；', '，', ',', '、', ' '],
            ));
            continue;
        }

        if current_len + line_len > max_len && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }

        current.push_str(line);
        current_len += line_len;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_by_delimiters(content: &str, max_len: usize, delimiters: &[char]) -> Vec<String> {
    let mut parts = split_with_delimiters(content, delimiters);
    if parts.len() <= 1 {
        parts = split_with_delimiters(content, &['，', ',', '、', ' ']);
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    for part in parts {
        let part_len = part.chars().count();
        let current_len = current.chars().count();

        if current_len + part_len <= max_len {
            current.push_str(&part);
            continue;
        }

        if !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }

        if part_len <= max_len {
            current.push_str(&part);
        } else {
            chunks.extend(split_long_token(&part, max_len));
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_with_delimiters(content: &str, delimiters: &[char]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        current.push(ch);
        if delimiters.contains(&ch) {
            parts.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    parts
}

fn split_long_token(content: &str, max_len: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        current.push(ch);
        if current.chars().count() >= max_len {
            chunks.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn parse_reply_items(content: &str) -> Result<Vec<NaturalReplyItem>> {
    let items: Vec<NaturalReplyItem> = serde_json::from_str(content)?;
    if items.is_empty() {
        return Err(Error::ValidationError(
            "LLM reply JSON array must not be empty".to_string(),
        ));
    }
    Ok(items)
}

fn convert_to_batches(
    items: Vec<NaturalReplyItem>,
    target_type: &str,
    max_one_reply_length: usize,
) -> Result<Vec<QQMessageBatch>> {
    let mut batches = Vec::new();

    for item in items {
        match item {
            NaturalReplyItem::PlainText { content } => {
                if content.trim().is_empty() {
                    return Err(Error::ValidationError(
                        "plain_text.content must not be empty".to_string(),
                    ));
                }
                for chunk in split_plain_text(&content, max_one_reply_length) {
                    batches.push(QQMessageBatch {
                        text_length: chunk.chars().count(),
                        messages: vec![Message::PlainText(PlainTextMessage { text: chunk })],
                    });
                }
            }
            NaturalReplyItem::At { target } => {
                if target_type != TARGET_TYPE_GROUP {
                    return Err(Error::ValidationError(
                        "top-level at is only allowed for group target".to_string(),
                    ));
                }
                let target = target.trim().to_string();
                if target.is_empty() {
                    return Err(Error::ValidationError(
                        "at.target must not be empty".to_string(),
                    ));
                }
                batches.push(QQMessageBatch {
                    text_length: 0,
                    messages: vec![Message::At(AtTargetMessage {
                        target: Some(target),
                    })],
                });
            }
            NaturalReplyItem::CombineText { content_list } => {
                if content_list.is_empty() {
                    return Err(Error::ValidationError(
                        "combine_text.content_list must not be empty".to_string(),
                    ));
                }

                let mut messages = Vec::new();
                let mut text_length = 0usize;
                let mut contains_plain_text = false;

                for content_item in content_list {
                    match content_item {
                        NaturalReplyContentItem::PlainText { content } => {
                            if content.is_empty() {
                                return Err(Error::ValidationError(
                                    "combine_text plain_text.content must not be empty".to_string(),
                                ));
                            }
                            let has_substantive_text = !content.trim().is_empty();
                            text_length += content.chars().count();
                            contains_plain_text |= has_substantive_text;
                            messages.push(Message::PlainText(PlainTextMessage { text: content }));
                        }
                        NaturalReplyContentItem::At { target } => {
                            if target_type != TARGET_TYPE_GROUP {
                                return Err(Error::ValidationError(
                                    "at inside combine_text is only allowed for group target"
                                        .to_string(),
                                ));
                            }
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

                if !contains_plain_text {
                    return Err(Error::ValidationError(
                        "combine_text must contain at least one plain_text item".to_string(),
                    ));
                }

                batches.push(QQMessageBatch {
                    messages,
                    text_length,
                });
            }
        }
    }

    Ok(batches)
}

pub fn infer_qq_reply_batches(
    llm_ref: &dyn LLMBase,
    content: &str,
    character_name: &str,
    target_id: &str,
    target_type: &str,
    style: &str,
    max_one_reply_length: usize,
    mention_target_id: Option<&str>,
) -> Result<QQNaturalLanguageReplyInference> {
    let messages = vec![
        OpenAIMessage::system(build_system_prompt(
            character_name,
            style,
            target_id,
            target_type,
            max_one_reply_length,
            mention_target_id,
        )),
        OpenAIMessage::user(content.to_string()),
    ];

    let reply = llm_ref.inference(&InferenceParam {
        messages: &messages,
        tools: None,
    });
    let raw_reply_content = reply
        .content
        .ok_or_else(|| Error::ValidationError("LLM response content is empty".to_string()))?;
    let items = parse_reply_items(&raw_reply_content)?;
    let batches = convert_to_batches(items, target_type, max_one_reply_length)?;

    Ok(QQNaturalLanguageReplyInference {
        raw_reply_content,
        batches,
    })
}

impl Node for NaturalLanguageReplyNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("调用 LLM 生成结构化 QQ 回复消息批次，不负责实际发送")
    }

    node_input![
        port! { name = "content", ty = String, desc = "待修改的原文草稿，将作为第一条 user 消息传给二次 LLM 做润色/改写" },
        port! { name = "character_name", ty = String, desc = "角色名字" },
        port! { name = "target_id", ty = String, desc = "目标 QQ 号或群号" },
        port! { name = "target_type", ty = String, desc = "目标类型：friend 或 group", optional },
        port! { name = "style", ty = String, desc = "语言风格或情绪" },
        port! { name = "max_one_reply_length", ty = Integer, desc = "单条 plain_text 最多字符数" },
        port! { name = "llm_ref", ty = LLModel, desc = "LLMBase 引用" },
    ];

    node_output![
        port! { name = "message_batches", ty = Vec(Vec(QQMessage)), desc = "LLM 生成的 QQ 消息批次列表" },
        port! { name = "raw_reply_json", ty = String, desc = "LLM 原始 JSON 回复字符串" },
        port! { name = "batch_count", ty = Integer, desc = "生成的消息批次数" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = match inputs.get("content") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("content is required".to_string())),
        };
        let character_name = match inputs.get("character_name") {
            Some(DataValue::String(value)) => value.clone(),
            _ => {
                return Err(Error::InvalidNodeInput(
                    "character_name is required".to_string(),
                ))
            }
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("target_id is required".to_string())),
        };
        let target_type = normalize_target_type(inputs.get("target_type"));
        let style = match inputs.get("style") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("style is required".to_string())),
        };
        let max_one_reply_length = match inputs.get("max_one_reply_length") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) => {
                return Err(Error::ValidationError(
                    "max_one_reply_length must be greater than 0".to_string(),
                ))
            }
            _ => {
                return Err(Error::InvalidNodeInput(
                    "max_one_reply_length is required".to_string(),
                ))
            }
        };
        let llm_ref = match inputs.get("llm_ref") {
            Some(DataValue::LLModel(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("llm_ref is required".to_string())),
        };

        let inference = infer_qq_reply_batches(
            llm_ref.as_ref(),
            &content,
            &character_name,
            &target_id,
            target_type,
            &style,
            max_one_reply_length,
            None,
        )?;

        let batch_values = inference
            .batches
            .iter()
            .map(|batch| {
                DataValue::Vec(
                    Box::new(DataType::QQMessage),
                    batch
                        .messages
                        .iter()
                        .cloned()
                        .map(DataValue::QQMessage)
                        .collect(),
                )
            })
            .collect();

        let mut outputs = HashMap::new();
        outputs.insert(
            "message_batches".to_string(),
            DataValue::Vec(
                Box::new(DataType::Vec(Box::new(DataType::QQMessage))),
                batch_values,
            ),
        );
        outputs.insert(
            "raw_reply_json".to_string(),
            DataValue::String(inference.raw_reply_content),
        );
        outputs.insert(
            "batch_count".to_string(),
            DataValue::Integer(inference.batches.len() as i64),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_system_prompt, infer_qq_reply_batches, normalize_target_type, split_plain_text,
        NaturalLanguageReplyNode, TARGET_TYPE_FRIEND, TARGET_TYPE_GROUP,
    };
    use crate::bot_adapter::models::message::Message;
    use crate::error::Result;
    use crate::llm::llm_base::LLMBase;
    use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct StubLlm {
        response: String,
    }

    impl LLMBase for StubLlm {
        fn get_model_name(&self) -> &str {
            "stub"
        }

        fn inference(&self, _param: &InferenceParam) -> OpenAIMessage {
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some(self.response.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }
        }
    }

    #[derive(Debug)]
    struct RecordingLlm {
        response: String,
        seen_messages: Mutex<Vec<OpenAIMessage>>,
    }

    impl LLMBase for RecordingLlm {
        fn get_model_name(&self) -> &str {
            "recording-stub"
        }

        fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
            *self
                .seen_messages
                .lock()
                .expect("recording llm mutex should lock") = param.messages.to_vec();
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some(self.response.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }
        }
    }

    fn base_inputs(llm_response: &str) -> HashMap<String, DataValue> {
        HashMap::from([
            ("content".to_string(), DataValue::String("你好".to_string())),
            (
                "character_name".to_string(),
                DataValue::String("测试角色".to_string()),
            ),
            (
                "target_id".to_string(),
                DataValue::String("123456".to_string()),
            ),
            (
                "target_type".to_string(),
                DataValue::String(TARGET_TYPE_GROUP.to_string()),
            ),
            ("style".to_string(), DataValue::String("温柔".to_string())),
            ("max_one_reply_length".to_string(), DataValue::Integer(5)),
            (
                "llm_ref".to_string(),
                DataValue::LLModel(Arc::new(StubLlm {
                    response: llm_response.to_string(),
                })),
            ),
        ])
    }

    #[test]
    fn target_type_port_is_optional_and_defaults_to_friend() {
        let node = NaturalLanguageReplyNode::new("n1", "reply");
        let port = node
            .input_ports()
            .into_iter()
            .find(|port| port.name == "target_type")
            .expect("target_type port should exist");
        assert!(!port.required);
        assert_eq!(normalize_target_type(None), TARGET_TYPE_FRIEND);
    }

    #[test]
    fn build_system_prompt_includes_group_mention_rules_when_target_provided() {
        let prompt = build_system_prompt(
            "测试角色",
            "温柔",
            "987654",
            TARGET_TYPE_GROUP,
            8,
            Some("123456"),
        );

        assert!(prompt.contains("默认只输出 1 个顶层数组元素"));
        assert!(prompt.contains("回复要保持简短自然"));
        assert!(prompt.contains("待修改的原文草稿"));
        assert!(prompt.contains("不要把这段原文当成别人刚发来的消息再回复一次"));
        assert!(prompt.contains("保留内容完整性"));
        assert!(prompt.contains("宁可分多条发送"));
        assert!(prompt.contains("不要凭空补充"));
        assert!(prompt.contains("本次优先回复给群成员 123456"));
        assert!(prompt.contains("plain_text(\" \")"));
        assert!(prompt.contains("@ 后面必须保留一个半角空格"));
    }

    #[test]
    fn build_system_prompt_omits_group_mention_rules_without_target() {
        let group_prompt =
            build_system_prompt("测试角色", "温柔", "987654", TARGET_TYPE_GROUP, 8, None);
        let friend_prompt = build_system_prompt(
            "测试角色",
            "温柔",
            "987654",
            TARGET_TYPE_FRIEND,
            8,
            Some("123456"),
        );

        assert!(!group_prompt.contains("群聊定向回复补充规则"));
        assert!(!friend_prompt.contains("群聊定向回复补充规则"));
    }

    #[test]
    fn infer_batches_splits_long_plain_text() -> Result<()> {
        let llm = StubLlm {
            response: r#"[{"message_type":"plain_text","content":"123456789"}]"#.to_string(),
        };
        let inference = infer_qq_reply_batches(
            &llm,
            "你好",
            "测试角色",
            "42",
            TARGET_TYPE_GROUP,
            "温柔",
            4,
            None,
        )?;
        let lengths: Vec<usize> = inference
            .batches
            .iter()
            .map(|batch| batch.text_length)
            .collect();
        assert_eq!(lengths, vec![4, 4, 1]);
        Ok(())
    }

    #[test]
    fn split_plain_text_prefers_newline_boundaries_for_code() {
        let chunks = split_plain_text(
            "line1 = 1\nline2 = 2\nline3 = 3\nline4 = 4\n",
            20,
        );

        assert_eq!(chunks, vec!["line1 = 1\nline2 = 2\n", "line3 = 3\nline4 = 4\n"]);
    }

    #[test]
    fn split_plain_text_prefers_sentence_boundaries() {
        let chunks = split_plain_text("第一句。第二句。第三句。", 8);

        assert_eq!(chunks, vec!["第一句。第二句。", "第三句。"]);
    }

    #[test]
    fn infer_batches_passes_group_mention_prompt_to_llm() -> Result<()> {
        let llm = RecordingLlm {
            response: r#"[{"message_type":"plain_text","content":"你好"}]"#.to_string(),
            seen_messages: Mutex::new(Vec::new()),
        };

        infer_qq_reply_batches(
            &llm,
            "你好",
            "测试角色",
            "987654",
            TARGET_TYPE_GROUP,
            "温柔",
            8,
            Some("123456"),
        )?;

        let seen_messages = llm
            .seen_messages
            .lock()
            .expect("recording llm mutex should lock")
            .clone();
        let system_prompt = seen_messages
            .first()
            .and_then(|message| message.content.as_deref())
            .expect("system prompt should exist");

        assert!(system_prompt.contains("本次优先回复给群成员 123456"));
        assert!(system_prompt.contains("默认优先使用 combine_text"));
        Ok(())
    }

    #[test]
    fn infer_batches_allows_space_segment_in_combine_text() -> Result<()> {
        let llm = StubLlm {
            response: r#"[{"message_type":"combine_text","content_list":[{"message_type":"at","target":"42"},{"message_type":"plain_text","content":" "},{"message_type":"plain_text","content":"你好"}]}]"#.to_string(),
        };

        let inference = infer_qq_reply_batches(
            &llm,
            "你好",
            "测试角色",
            "987654",
            TARGET_TYPE_GROUP,
            "温柔",
            8,
            Some("42"),
        )?;

        assert_eq!(inference.batches.len(), 1);
        assert!(matches!(
            inference.batches[0].messages.as_slice(),
            [
                Message::At(_),
                Message::PlainText(space),
                Message::PlainText(text),
            ] if space.text == " " && text.text == "你好"
        ));
        Ok(())
    }

    #[test]
    fn infer_batches_rejects_at_for_friend_target() {
        let llm = StubLlm {
            response: r#"[{"message_type":"at","target":"42"}]"#.to_string(),
        };
        let err = infer_qq_reply_batches(
            &llm,
            "你好",
            "测试角色",
            "42",
            TARGET_TYPE_FRIEND,
            "温柔",
            8,
            None,
        )
        .expect_err("friend target should reject at");
        assert!(err.to_string().contains("only allowed for group"));
    }

    #[test]
    fn execute_outputs_nested_message_batches() -> Result<()> {
        let mut node = NaturalLanguageReplyNode::new("reply", "Reply");
        let outputs = node.execute(base_inputs(
            r#"[{"message_type":"combine_text","content_list":[{"message_type":"at","target":"123456"},{"message_type":"plain_text","content":"你好"}]}]"#,
        ))?;

        match outputs.get("message_batches") {
            Some(DataValue::Vec(batch_ty, batches)) => {
                assert_eq!(
                    batch_ty.as_ref(),
                    &DataType::Vec(Box::new(DataType::QQMessage))
                );
                assert_eq!(batches.len(), 1);
            }
            other => panic!("unexpected message_batches output: {other:?}"),
        }
        assert!(matches!(
            outputs.get("raw_reply_json"),
            Some(DataValue::String(_))
        ));
        Ok(())
    }
}
