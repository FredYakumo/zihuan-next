use crate::{DataType, DataValue, Node, NodeConfigField};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use zihuan_core::error::Result;

/// Node factory function type
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;
pub type RegistryInitFn = fn() -> Result<()>;

/// Global node registry
pub struct NodeRegistry {
    factories: RwLock<HashMap<String, NodeFactory>>,
    metadata: RwLock<HashMap<String, NodeTypeMetadata>>,
}

#[derive(Debug, Clone)]
pub struct NodeTypeMetadata {
    pub type_id: String,
    pub display_name: String,
    pub category: String,
    pub description: String,
}

impl NodeRegistry {
    fn new() -> Self {
        Self {
            factories: RwLock::new(HashMap::new()),
            metadata: RwLock::new(HashMap::new()),
        }
    }

    /// Register a node type with its factory function
    pub fn register(
        &self,
        type_id: impl Into<String>,
        display_name: impl Into<String>,
        category: impl Into<String>,
        description: impl Into<String>,
        factory: NodeFactory,
    ) -> Result<()> {
        let type_id = type_id.into();
        let metadata = NodeTypeMetadata {
            type_id: type_id.clone(),
            display_name: display_name.into(),
            category: category.into(),
            description: description.into(),
        };

        self.factories
            .write()
            .unwrap()
            .insert(type_id.clone(), factory);
        self.metadata.write().unwrap().insert(type_id, metadata);
        Ok(())
    }

    /// Create a new node instance by type ID
    pub fn create_node(
        &self,
        type_id: &str,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Box<dyn Node>> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "Node type '{}' not registered",
                type_id
            ))
        })?;

        Ok(factory(id.into(), name.into()))
    }

    /// Return the canonical input and output ports for a registered node type.
    ///
    /// Returns `None` if the type is not registered.
    pub fn get_node_ports(&self, type_id: &str) -> Option<(Vec<crate::Port>, Vec<crate::Port>)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((node.input_ports(), node.output_ports()))
    }

    pub fn get_node_dynamic_port_flags(&self, type_id: &str) -> Option<(bool, bool)> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some((
            node.has_dynamic_input_ports(),
            node.has_dynamic_output_ports(),
        ))
    }

    pub fn get_node_config_fields(&self, type_id: &str) -> Option<Vec<NodeConfigField>> {
        let factories = self.factories.read().unwrap();
        let factory = factories.get(type_id)?;
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        Some(node.config_fields())
    }

    /// Legacy compatibility flag. EventProducer has been removed, so this
    /// always returns false.
    pub fn is_event_producer(&self, _type_id: &str) -> bool {
        false
    }

    /// Get all registered node types
    pub fn get_all_types(&self) -> Vec<NodeTypeMetadata> {
        self.metadata.read().unwrap().values().cloned().collect()
    }

    /// Get node types by category
    pub fn get_types_by_category(&self, category: &str) -> Vec<NodeTypeMetadata> {
        self.metadata
            .read()
            .unwrap()
            .values()
            .filter(|meta| meta.category == category)
            .cloned()
            .collect()
    }

    /// Get all categories
    pub fn get_categories(&self) -> Vec<String> {
        let mut categories: Vec<_> = self
            .metadata
            .read()
            .unwrap()
            .values()
            .map(|meta| meta.category.clone())
            .collect();
        categories.sort();
        categories.dedup();
        categories
    }
}

/// Global singleton registry
pub static NODE_REGISTRY: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);

/// Helper macro to register a node type
#[macro_export]
macro_rules! register_node {
    ($type_id:expr, $display_name:expr, $category:expr, $description:expr, $node_struct:ty) => {
        $crate::registry::NODE_REGISTRY
            .register(
                $type_id,
                $display_name,
                $category,
                $description,
                std::sync::Arc::new(|id: String, name: String| {
                    Box::new(<$node_struct>::new(id, name))
                }),
            )
            .unwrap();
    };
}

pub fn build_node_graph_from_definition(
    definition: &crate::graph_io::NodeGraphDefinition,
) -> Result<crate::NodeGraph> {
    let mut graph = crate::NodeGraph::new();
    graph.set_definition(definition.clone());

    if !definition.edges.is_empty() {
        graph.set_edges(definition.edges.clone());
    }

    for node_def in &definition.nodes {
        let node = NODE_REGISTRY.create_node(
            &node_def.node_type,
            node_def.id.clone(),
            node_def.name.clone(),
        )?;

        // Parse inline values
        if !node_def.inline_values.is_empty() {
            let mut values = HashMap::new();
            let ports: HashMap<String, DataType> = node
                .input_ports()
                .into_iter()
                .chain(node.output_ports().into_iter())
                .map(|p| (p.name, p.data_type))
                .chain(
                    node.config_fields()
                        .into_iter()
                        .map(|field| (field.key, field.data_type)),
                )
                .collect();

            for (port_name, json_val) in &node_def.inline_values {
                if let Some(data_type) = ports.get(port_name) {
                    if let Some(val) = json_to_data_value(json_val, data_type) {
                        values.insert(port_name.clone(), val);
                    }
                }
            }
            if !values.is_empty() {
                graph.inline_values.insert(node_def.id.clone(), values);
            }
        }

        graph.add_node(node)?;
    }

    let inline_values_snapshot = graph.inline_values.clone();
    for (node_id, node) in graph.nodes.iter_mut() {
        if let Some(inline_values) = inline_values_snapshot.get(node_id) {
            node.apply_inline_config(inline_values)?;
        }
    }

    // Second pass: nodes with dynamic input ports (e.g. FormatStringNode) only expose
    // their full port list after apply_inline_config. Re-collect any inline values that
    // were skipped in the first pass because the ports didn't exist yet.
    let extra_inline: Vec<(String, HashMap<String, DataValue>)> = definition
        .nodes
        .iter()
        .filter_map(|node_def| {
            if node_def.inline_values.is_empty() {
                return None;
            }
            let node = graph.nodes.get(&node_def.id)?;
            let already_set: std::collections::HashSet<&str> = graph
                .inline_values
                .get(&node_def.id)
                .map(|m| m.keys().map(String::as_str).collect())
                .unwrap_or_default();
            let ports: HashMap<String, DataType> = node
                .input_ports()
                .into_iter()
                .chain(node.output_ports().into_iter())
                .map(|p| (p.name, p.data_type))
                .chain(
                    node.config_fields()
                        .into_iter()
                        .map(|field| (field.key, field.data_type)),
                )
                .collect();
            let mut extra = HashMap::new();
            for (port_name, json_val) in &node_def.inline_values {
                if !already_set.contains(port_name.as_str()) {
                    if let Some(data_type) = ports.get(port_name) {
                        if let Some(val) = json_to_data_value(json_val, data_type) {
                            extra.insert(port_name.clone(), val);
                        }
                    }
                }
            }
            if extra.is_empty() {
                None
            } else {
                Some((node_def.id.clone(), extra))
            }
        })
        .collect();
    for (node_id, extra_values) in extra_inline {
        graph
            .inline_values
            .entry(node_id)
            .or_default()
            .extend(extra_values);
    }

    let runtime_variable_store = graph.runtime_variable_store();
    graph.set_runtime_variable_store(runtime_variable_store);

    Ok(graph)
}

pub(crate) fn json_to_data_value(json: &Value, target_type: &DataType) -> Option<DataValue> {
    match (json, target_type) {
        (_, DataType::Any) => infer_any_data_value(json),
        (Value::String(s), DataType::String) => Some(DataValue::String(s.clone())),
        (Value::String(s), DataType::Password) => Some(DataValue::Password(s.clone())),
        (Value::String(s), DataType::Boolean) => {
            if s == "true" {
                Some(DataValue::Boolean(true))
            } else if s == "false" {
                Some(DataValue::Boolean(false))
            } else {
                None
            }
        }
        (Value::String(s), DataType::Integer) => s.parse().ok().map(DataValue::Integer),
        (Value::String(s), DataType::Float) => s.parse().ok().map(DataValue::Float),
        (Value::String(s), DataType::Json) => match serde_json::from_str(s) {
            Ok(v) => Some(DataValue::Json(v)),
            Err(_) => Some(DataValue::String(s.clone())), // Fallback? or Error? Or maybe just create Json string
        },

        (Value::Number(n), DataType::Integer) => n.as_i64().map(DataValue::Integer),
        (Value::Number(n), DataType::Float) => n.as_f64().map(DataValue::Float),

        (Value::Bool(b), DataType::Boolean) => Some(DataValue::Boolean(*b)),

        (v, DataType::Json) => Some(DataValue::Json(v.clone())),

        (Value::Array(items), DataType::Vector) => items
            .iter()
            .map(|item| match item {
                Value::Number(value) => value.as_f64().map(|v| v as f32),
                Value::String(value) => value.parse::<f32>().ok(),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(DataValue::Vector),

        // Single OpenAIMessage from a JSON object: {"role": "user", "content": "..."}
        (Value::Object(map), DataType::OpenAIMessage) => {
            fn parse_role(v: &Value) -> zihuan_core::llm::MessageRole {
                let s = v.as_str().unwrap_or("user").to_ascii_lowercase();
                match s.as_str() {
                    "system" => zihuan_core::llm::MessageRole::System,
                    "assistant" => zihuan_core::llm::MessageRole::Assistant,
                    "tool" => zihuan_core::llm::MessageRole::Tool,
                    _ => zihuan_core::llm::MessageRole::User,
                }
            }

            let role = map
                .get("role")
                .map(|v| parse_role(v))
                .unwrap_or(zihuan_core::llm::MessageRole::User);
            let content = match map.get("content") {
                Some(Value::Null) | None => None,
                Some(other) => {
                    serde_json::from_value::<zihuan_core::llm::MessageContent>(other.clone()).ok()
                }
            };
            Some(DataValue::OpenAIMessage(zihuan_core::llm::OpenAIMessage {
                role,
                content,
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
            }))
        }

        (_, DataType::Sender) => serde_json::from_value::<
            zihuan_core::ims_bot_adapter::models::sender_model::Sender,
        >(json.clone())
        .ok()
        .map(DataValue::Sender),

        // Single QQ Message from a JSON object: {"type": "text", "data": {"text": "..."}}
        (_, DataType::QQMessage) => serde_json::from_value::<
            zihuan_core::ims_bot_adapter::models::message::Message,
        >(json.clone())
        .ok()
        .map(DataValue::QQMessage),

        // Single Image payload from a JSON object.
        (_, DataType::Image) => {
            serde_json::from_value::<crate::data_value::ImageData>(json.clone())
                .ok()
                .map(DataValue::Image)
        }

        // Generic Vec: recurse per element using the inner type.
        // Handles Vec<OpenAIMessage>, Vec<QQMessage>, and any other Vec<X>.
        (Value::Array(items), DataType::Vec(inner)) => {
            let parsed: Vec<DataValue> = items
                .iter()
                .filter_map(|item| json_to_data_value(item, inner))
                .collect();
            Some(DataValue::Vec(inner.clone(), parsed))
        }

        _ => None,
    }
}

fn infer_any_data_value(json: &Value) -> Option<DataValue> {
    match json {
        Value::String(s) => Some(DataValue::String(s.clone())),
        Value::Number(n) => n
            .as_i64()
            .map(DataValue::Integer)
            .or_else(|| n.as_f64().map(DataValue::Float)),
        Value::Bool(b) => Some(DataValue::Boolean(*b)),
        _ => Some(DataValue::Json(json.clone())),
    }
}

/// Register all node types that live within this crate.
/// Called by the main binary's `init_registry::init_node_registry` and also by
/// in-crate tests that need the registry populated.
pub fn init_node_registry() -> zihuan_core::error::Result<()> {
    use crate::util::{
        AndThenNode, ArrayGetNode, AtQQTargetMessageNode, BinaryToImageContentPartNode,
        BooleanBranchNode, BooleanNotNode, BuildMultimodalUserMessageNode, ConcatVecNode,
        ConditionalNode, ConditionalRouterNode, CurrentTimeNode, FormatStringNode,
        FunctionInputsNode, FunctionNode, FunctionOutputsNode, GraphInputsNode, GraphOutputsNode,
        JoinStringNode, JsonExtractNode, JsonParserNode, JsonToQQMessageVecNode,
        MessageContentNode, MessageListDataNode, OpenAIMessageContentAsJsonNode,
        OpenAIMessageSessionCacheClearNode, OpenAIMessageSessionCacheGetNode,
        OpenAIMessageSessionCacheNode, OpenAIMessageSessionCacheSetNode, OpenAIMessageToStringNode,
        PreviewMessageListNode, PreviewQQMessageListNode, PreviewStringNode, PushBackVecNode,
        QQMessageJsonOutputSystemPromptProviderNode, QQMessageListDataNode, QQMessageToImageNode,
        SessionStateClearNode, SessionStateGetNode, SessionStateReleaseNode,
        SessionStateTryClaimNode, SetVariableNode, StackNode, StringDataNode, StringIsNotEmptyNode,
        StringToImageContentPartNode, StringToOpenAIMessageNode, StringToPlainTextNode, SwitchNode,
        ToolResultNode,
    };

    register_node!(
        "and_then",
        "And Then",
        "工具",
        "等待两个输入都到齐后，原样透传第二个输入",
        AndThenNode
    );
    register_node!(
        "format_string",
        "格式化字符串",
        "工具",
        "通过 ${变量名} 模板语法将输入变量格式化为字符串",
        FormatStringNode
    );
    register_node!(
        "function",
        "函数",
        "工具",
        "执行节点私有函数子图，输入输出端口由函数签名动态决定",
        FunctionNode
    );
    register_node!(
        "function_inputs",
        "函数输入",
        "内部",
        "函数子图内部边界节点，将调用参数展开为动态输出端口",
        FunctionInputsNode
    );
    register_node!(
        "function_outputs",
        "函数输出",
        "内部",
        "函数子图内部边界节点，汇总子图结果作为函数返回值",
        FunctionOutputsNode
    );
    register_node!(
        "graph_inputs",
        "节点图输入",
        "内部",
        "主节点图内部边界节点，将运行时参数展开为动态输出端口",
        GraphInputsNode
    );
    register_node!(
        "graph_outputs",
        "节点图输出",
        "内部",
        "主节点图内部边界节点，汇总主图结果作为返回值",
        GraphOutputsNode
    );
    register_node!(
        "conditional",
        "条件分支",
        "工具",
        "根据条件选择不同的输出分支",
        ConditionalNode
    );
    register_node!(
        "conditional_router",
        "变量分拣器",
        "工具",
        "按布尔条件在两路输入间选择一路输出，适合循环状态切换",
        ConditionalRouterNode
    );
    register_node!(
        "switch_gate",
        "开关器",
        "工具",
        "当 enabled 为 true 时透传输入，否则阻断后续数据流",
        SwitchNode
    );
    register_node!(
        "set_variable",
        "设置变量",
        "工具",
        "将输入值写入运行期节点图变量，变量会在每次重新运行时回到初始值",
        SetVariableNode
    );
    register_node!(
        "boolean_branch",
        "布尔分路",
        "工具",
        "根据 condition 将 input 送到 true 或 false 分支，未选中的分支不会输出",
        BooleanBranchNode
    );
    register_node!(
        "boolean_not",
        "布尔取反",
        "工具",
        "对输入的 Boolean 值取反",
        BooleanNotNode
    );
    register_node!(
        "array_get",
        "列表取元素",
        "工具",
        "从列表中按下标取元素，支持负数下标（-1为最后一个）",
        ArrayGetNode
    );
    register_node!(
        "stack",
        "封装元素为数组",
        "工具",
        "将单个元素封装为单元素 List",
        StackNode
    );
    register_node!(
        "concat_vec",
        "拼接两个列表",
        "工具",
        "将 vec2 拼接到 vec1 后面，要求两个列表的元素类型一致",
        ConcatVecNode
    );
    register_node!(
        "join_string",
        "拼接字符串列表",
        "工具",
        "使用分隔符将 Vec<String> 拼接为单个字符串",
        JoinStringNode
    );
    register_node!(
        "push_back_vec",
        "列表尾部追加元素",
        "工具",
        "将单个元素追加到列表末尾，要求元素类型与列表元素类型一致",
        PushBackVecNode
    );
    register_node!(
        "json_parser",
        "JSON解析器",
        "工具",
        "将JSON字符串解析为结构化数据",
        JsonParserNode
    );
    register_node!(
        "json_extract",
        "提取 JSON 字段",
        "工具",
        "通过字段编辑器配置要提取的字段列表，并动态输出对应类型的字段值",
        JsonExtractNode
    );
    register_node!(
        "message_content",
        "提取 OpenAIMessage 内容",
        "消息",
        "从 OpenAIMessage 中提取 content 字段，以字符串形式输出",
        MessageContentNode
    );
    register_node!(
        "string_to_openai_message",
        "字符串转 OpenAIMessage",
        "消息",
        "将字符串封装为可选 role 的 OpenAIMessage",
        StringToOpenAIMessageNode
    );
    register_node!(
        "openai_message_content_as_json",
        "OpenAIMessage内容转JSON",
        "消息",
        "将 OpenAIMessage 的 content 字符串解析为 JSON",
        OpenAIMessageContentAsJsonNode
    );
    register_node!(
        "openai_message_to_string",
        "OpenAIMessage转字符串",
        "消息",
        "将 OpenAIMessage 的 reasoning_content（如有）与 content 拼接为字符串",
        OpenAIMessageToStringNode
    );
    register_node!(
        "as_system_openai_message",
        "字符串转 OpenAIMessage",
        "消息",
        "兼容旧节点类型 ID：将字符串封装为可选 role 的 OpenAIMessage，默认 role=system",
        StringToOpenAIMessageNode
    );
    register_node!(
        "preview_string",
        "Preview String",
        "工具",
        "在节点卡片内预览输入字符串",
        PreviewStringNode
    );
    register_node!(
        "string_data",
        "String Data",
        "数据",
        "字符串数据源，通过UI输入框提供字符串",
        StringDataNode
    );
    register_node!(
        "string_is_not_empty",
        "字符串非空判断",
        "工具",
        "判断字符串是否非空，可选 trim_before_check 决定是否先 trim 再判断",
        StringIsNotEmptyNode
    );
    register_node!(
        "current_time",
        "当前时间",
        "数据",
        "输出当前本地时间字符串，无需输入",
        CurrentTimeNode
    );
    register_node!(
        "preview_message_list",
        "Preview OpenAIMessage List",
        "工具",
        "在节点卡片内预览 OpenAIMessage 列表",
        PreviewMessageListNode
    );
    register_node!(
        "qq_message_preview",
        "Preview QQ Messages",
        "工具",
        "在节点卡片内实时预览 QQMessage 列表（含图片）",
        PreviewQQMessageListNode
    );
    register_node!(
        "message_list_data",
        "OpenAIMessage List Data",
        "数据",
        "OpenAIMessage 列表数据源，通过 UI 容器编辑器提供列表数据",
        MessageListDataNode
    );
    register_node!(
        "qq_message_list_data",
        "QQMessageList Data",
        "数据",
        "QQ消息列表数据源，通过UI容器编辑器提供QQMessageList",
        QQMessageListDataNode
    );
    register_node!(
        "string_to_plain_text",
        "字符串转QQ纯文本",
        "消息",
        "将字符串转换为 QQ 消息中的纯文本（PlainText）消息段",
        StringToPlainTextNode
    );
    register_node!(
        "at_qq_target_message",
        "构造QQAt消息",
        "消息",
        "输入 QQ 目标 id 字符串，输出 @ 目标的 QQ 消息段",
        AtQQTargetMessageNode
    );
    register_node!(
        "qq_message_to_image",
        "QQ消息转图片数据",
        "消息",
        "将 QQMessage(Image) 转为 Image 数据，并输出对象存储路径",
        QQMessageToImageNode
    );
    register_node!(
        "json_to_qq_message_vec",
        "JSON转QQMessage列表",
        "消息",
        "将 LLM 输出的 QQ 消息 JSON 二维数组转换为 Vec<Vec<QQMessage>>",
        JsonToQQMessageVecNode
    );
    register_node!(
        "tool_result",
        "Tool 结果消息",
        "AI",
        "将工具执行结果封装为 role=tool 的 OpenAIMessage，供 agentic loop 回写对话列表",
        ToolResultNode
    );
    register_node!(
        "openai_message_session_cache",
        "OpenAIMessage 会话暂存",
        "消息存储",
        "根据缓存 Ref、sender_id 和消息列表向当前运行期会话历史追加 Vec<OpenAIMessage>",
        OpenAIMessageSessionCacheNode
    );
    register_node!(
        "openai_message_session_cache_get",
        "获取 OpenAIMessage 历史",
        "消息存储",
        "根据 OpenAIMessage 会话缓存 Ref 和 sender_id 读取当前运行期累计的 Vec<OpenAIMessage>",
        OpenAIMessageSessionCacheGetNode
    );
    register_node!("openai_message_session_cache_set", "覆写 OpenAIMessage 历史", "消息存储", "根据 OpenAIMessage 会话缓存 Ref、sender_id 和消息列表覆写当前运行期累计的 Vec<OpenAIMessage>", OpenAIMessageSessionCacheSetNode);
    register_node!(
        "openai_message_session_cache_clear",
        "清空 OpenAIMessage 历史",
        "消息存储",
        "根据 OpenAIMessage 会话缓存 Ref 和 sender_id 清空当前运行期累计的历史消息",
        OpenAIMessageSessionCacheClearNode
    );
    register_node!(
        "session_state_get",
        "读取会话状态",
        "消息存储",
        "读取 sender_id 当前是否处于会话中以及附加状态",
        SessionStateGetNode
    );
    register_node!(
        "session_state_clear",
        "清除会话状态",
        "消息存储",
        "清除 sender_id 当前会话状态",
        SessionStateClearNode
    );
    register_node!(
        "session_state_try_claim",
        "尝试占用会话",
        "消息存储",
        "原子检查并占用 sender_id 会话状态",
        SessionStateTryClaimNode
    );
    register_node!(
        "session_state_release",
        "释放会话占用",
        "消息存储",
        "释放 sender_id 当前持有的会话占用",
        SessionStateReleaseNode
    );
    register_node!(
        "qq_message_json_output_system_prompt_provider",
        "QQ消息JSON输出格式Prompt",
        "消息",
        "输出固定的 system prompt，要求 LLM 只返回 QQ 消息二维 JSON 数组",
        QQMessageJsonOutputSystemPromptProviderNode
    );
    register_node!(
        "string_to_image_content_part",
        "字符串转图片/视频 ContentPart",
        "消息",
        "将字符串 URL（或 data: URL）封装为 LLM 多模态 ContentPart，用于装配多模态 OpenAIMessage",
        StringToImageContentPartNode
    );
    register_node!(
        "binary_to_image_content_part",
        "二进制转图片/视频 ContentPart",
        "消息",
        "将二进制字节 + MIME 编码为 base64 data URL，并封装为 LLM 多模态 ContentPart",
        BinaryToImageContentPartNode
    );
    register_node!(
        "build_multimodal_user_message",
        "构建多模态 OpenAIMessage",
        "消息",
        "将可选文本和若干 ContentPart 拼接为多模态 OpenAIMessage，下游 LLM 推理节点直接消费",
        BuildMultimodalUserMessageNode
    );

    Ok(())
}

pub fn init_node_registry_with_extensions(extra_registrars: &[RegistryInitFn]) -> Result<()> {
    init_node_registry()?;
    for init in extra_registrars {
        init()?;
    }
    Ok(())
}
