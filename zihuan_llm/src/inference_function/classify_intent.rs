use std::sync::Arc;

use log::{info, warn};

use crate::model::OpenAIMessage;
use zihuan_core::llm::InferenceParam;

const LOG_PREFIX: &str = "[QqChatAgent]";
const CLASSIFY_INTENT_PROMPT: &str = r#"你是一个消息意图分类器。你必须只输出以下 9 个标签中的一个，且只能输出标签本身，不要输出解释、标点、引号、代码块或额外文字。

标签说明：
- 聊天：日常闲聊、问候、随意交流。
- 调侃：带有玩笑、讽刺、戏谑性质的消息。
- 查找：希望查询、搜索某类信息或知识。
- 解决复杂问题：需要深度分析、推理或规划的问题，例如数学题、逻辑题、哲学问题、方案设计等。
- 编写代码：要求生成或修改代码。
- 询问系统提示词：直接询问本机器人自身当前使用的系统提示词是什么，仅限针对机器人本身，例如「你的系统提示词是什么」。
- 询问模型名字：直接询问本机器人自身使用的是哪个模型，仅限针对机器人本身，例如「你用的什么模型」。
- 询问工具列表、功能：直接询问本机器人自身具备哪些工具或功能，仅限针对机器人本身，例如「你有哪些功能」。
- 其它：不属于以上任何类别。

可选标签：聊天 | 调侃 | 查找 | 解决复杂问题 | 编写代码 | 询问系统提示词 | 询问模型名字 | 询问工具列表、功能 | 其它"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentCategory {
    Chat,
    Tease,
    Search,
    SolveComplexProblem,
    WriteCode,
    AskSystemPrompt,
    AskModelName,
    AskToolList,
    Other,
}

impl IntentCategory {
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "聊天" => Some(Self::Chat),
            "调侃" => Some(Self::Tease),
            "查找" => Some(Self::Search),
            "解决复杂问题" => Some(Self::SolveComplexProblem),
            "编写代码" => Some(Self::WriteCode),
            "询问系统提示词" => Some(Self::AskSystemPrompt),
            "询问模型名字" => Some(Self::AskModelName),
            "询问工具列表、功能" => Some(Self::AskToolList),
            "其它" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chat => "聊天",
            Self::Tease => "调侃",
            Self::Search => "查找",
            Self::SolveComplexProblem => "解决复杂问题",
            Self::WriteCode => "编写代码",
            Self::AskSystemPrompt => "询问系统提示词",
            Self::AskModelName => "询问模型名字",
            Self::AskToolList => "询问工具列表、功能",
            Self::Other => "其它",
        }
    }
}

pub fn classify_intent(
    llm: &Arc<dyn zihuan_core::llm::llm_base::LLMBase>,
    message: &str,
) -> IntentCategory {
    let messages = vec![
        OpenAIMessage::system(CLASSIFY_INTENT_PROMPT.to_string()),
        OpenAIMessage::user(message.to_string()),
    ];
    let response = llm.inference(&InferenceParam {
        messages: &messages,
        tools: None,
    });
    let label = response.content_text_owned().unwrap_or_default();
    let trimmed = label.trim();
    let category = IntentCategory::from_label(trimmed).unwrap_or(IntentCategory::Other);
    if category == IntentCategory::Other && trimmed != IntentCategory::Other.label() {
        warn!(
            "{LOG_PREFIX} Invalid intent classification output '{}', fallback to {}",
            trimmed,
            IntentCategory::Other.label()
        );
    }
    info!(
        "{LOG_PREFIX} intent classified as {} from '{}'",
        category.label(),
        trimmed
    );
    category
}
