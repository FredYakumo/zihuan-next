use zihuan_core::error::Result;
use crate::{DataType, DataValue, Node};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Node factory function type
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;

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
            zihuan_core::error::Error::ValidationError(format!("Node type '{}' not registered", type_id))
        })?;

        Ok(factory(id.into(), name.into()))
    }

    /// Return the canonical input and output ports for a registered node type.
    /// Returns `None` if the type is not registered.
    pub fn get_node_ports(
        &self,
        type_id: &str,
    ) -> Option<(Vec<crate::Port>, Vec<crate::Port>)> {
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

    /// Returns true if the registered node type is an EventProducer.
    pub fn is_event_producer(&self, type_id: &str) -> bool {
        let factories = self.factories.read().unwrap();
        let Some(factory) = factories.get(type_id) else {
            return false;
        };
        let node = factory("__probe__".to_string(), "__probe__".to_string());
        node.node_type() == crate::NodeType::EventProducer
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

/// Build a NodeGraph from a NodeGraphDefinition
pub fn build_node_graph_from_definition(
    definition: &crate::graph_io::NodeGraphDefinition,
) -> Result<crate::NodeGraph> {
    let mut graph = crate::NodeGraph::new();
    graph.set_definition(definition.clone());

    if !definition.edges.is_empty() {
        graph.set_edges(definition.edges.clone());
    }

    // Create all nodes
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

        // Single OpenAIMessage from a JSON object: {"role": "user", "content": "..."}
        (Value::Object(map), DataType::OpenAIMessage) => {
            fn parse_role(v: &Value) -> zihuan_llm_types::MessageRole {
                let s = v.as_str().unwrap_or("user").to_ascii_lowercase();
                match s.as_str() {
                    "system" => zihuan_llm_types::MessageRole::System,
                    "assistant" => zihuan_llm_types::MessageRole::Assistant,
                    "tool" => zihuan_llm_types::MessageRole::Tool,
                    _ => zihuan_llm_types::MessageRole::User,
                }
            }

            let role = map
                .get("role")
                .map(|v| parse_role(v))
                .unwrap_or(zihuan_llm_types::MessageRole::User);
            let content = match map.get("content") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(other) => Some(other.to_string()),
            };
            Some(DataValue::OpenAIMessage(zihuan_llm_types::OpenAIMessage {
                role,
                content,
                tool_calls: Vec::new(),
                tool_call_id: None,
            }))
        }

        // Single QQ Message from a JSON object: {"type": "text", "data": {"text": "..."}}
        (_, DataType::QQMessage) => {
            serde_json::from_value::<zihuan_bot_types::message::Message>(json.clone())
                .ok()
                .map(DataValue::QQMessage)
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
    use crate::database::{MySqlNode, RedisNode};
    use crate::message_cache::MessageCacheNode;
    use crate::message_mysql_get_group_history::MessageMySQLGetGroupHistoryNode;
    use crate::message_mysql_get_user_history::MessageMySQLGetUserHistoryNode;
    use crate::message_mysql_persistence::MessageMySQLPersistenceNode;
    use crate::qq_message_list_mysql_persistence::QQMessageListMySQLPersistenceNode;
    use crate::util::{
        AndThenNode, ArrayGetNode, AtQQTargetMessageNode, BooleanBranchNode, BooleanNotNode,
        ConcatVecNode, ConditionalNode, ConditionalRouterNode, CurrentTimeNode, FormatStringNode,
        FunctionInputsNode, FunctionNode, FunctionOutputsNode, JoinStringNode, JsonExtractNode,
        JsonParserNode, JsonToQQMessageVecNode, LoopBreakNode, LoopNode, LoopStateUpdateNode,
        MessageContentNode, MessageListDataNode, OpenAIMessageContentAsJsonNode,
        OpenAIMessageSessionCacheClearNode, OpenAIMessageSessionCacheGetNode,
        OpenAIMessageSessionCacheNode, OpenAIMessageSessionCacheProviderNode,
        OpenAIMessageSessionCacheSetNode, PreviewMessageListNode, PreviewStringNode,
        PushBackVecNode, QQMessageJsonOutputSystemPromptProviderNode, QQMessageListDataNode,
        SessionStateClearNode, SessionStateGetNode, SessionStateProviderNode,
        SessionStateReleaseNode, SessionStateTryClaimNode, SetVariableNode, StackNode,
        StringDataNode, StringIsNotEmptyNode, StringToOpenAIMessageNode, StringToPlainTextNode, SwitchNode,
        ToolResultNode,
    };

    register_node!("and_then", "And Then", "工具", "等待两个输入都到齐后，原样透传第二个输入", AndThenNode);
    register_node!("format_string", "格式化字符串", "工具", "通过 ${变量名} 模板语法将输入变量格式化为字符串", FormatStringNode);
    register_node!("function", "函数", "工具", "执行节点私有函数子图，输入输出端口由函数签名动态决定", FunctionNode);
    register_node!("function_inputs", "函数输入", "内部", "函数子图内部边界节点，将调用参数展开为动态输出端口", FunctionInputsNode);
    register_node!("function_outputs", "函数输出", "内部", "函数子图内部边界节点，汇总子图结果作为函数返回值", FunctionOutputsNode);
    register_node!("conditional", "条件分支", "工具", "根据条件选择不同的输出分支", ConditionalNode);
    register_node!("conditional_router", "变量分拣器", "工具", "按布尔条件在两路输入间选择一路输出，适合循环状态切换", ConditionalRouterNode);
    register_node!("switch_gate", "开关器", "工具", "当 enabled 为 true 时透传输入，否则阻断后续数据流", SwitchNode);
    register_node!("set_variable", "设置变量", "工具", "将输入值写入运行期节点图变量，变量会在每次重新运行时回到初始值", SetVariableNode);
    register_node!("boolean_branch", "布尔分路", "工具", "根据 condition 将 input 送到 true 或 false 分支，未选中的分支不会输出", BooleanBranchNode);
    register_node!("boolean_not", "布尔取反", "工具", "对输入的 Boolean 值取反", BooleanNotNode);
    register_node!("loop", "循环", "工具", "重复执行，将 input 透传为 output，直到 LoopBreakNode 触发退出条件", LoopNode);
    register_node!("loop_break", "循环退出", "工具", "当 condition 为 true 时，通知循环节点在下一轮退出；放置在循环链路最末端", LoopBreakNode);
    register_node!("loop_state_update", "循环状态更新", "工具", "将 new_state 写入循环控制引用；循环下一轮将以此值为 output 输出，无需图中回边", LoopStateUpdateNode);
    register_node!("array_get", "列表取元素", "工具", "从列表中按下标取元素，支持负数下标（-1为最后一个）", ArrayGetNode);
    register_node!("stack", "封装元素为数组", "工具", "将单个元素封装为单元素 List", StackNode);
    register_node!("concat_vec", "拼接两个列表", "工具", "将 vec2 拼接到 vec1 后面，要求两个列表的元素类型一致", ConcatVecNode);
    register_node!("join_string", "拼接字符串列表", "工具", "使用分隔符将 Vec<String> 拼接为单个字符串", JoinStringNode);
    register_node!("push_back_vec", "列表尾部追加元素", "工具", "将单个元素追加到列表末尾，要求元素类型与列表元素类型一致", PushBackVecNode);
    register_node!("json_parser", "JSON解析器", "工具", "将JSON字符串解析为结构化数据", JsonParserNode);
    register_node!("json_extract", "提取 JSON 字段", "工具", "通过字段编辑器配置要提取的字段列表，并动态输出对应类型的字段值", JsonExtractNode);
    register_node!("message_content", "提取 OpenAIMessage 内容", "消息", "从 OpenAIMessage 中提取 content 字段，以字符串形式输出", MessageContentNode);
    register_node!("string_to_openai_message", "字符串转 OpenAIMessage", "消息", "将字符串封装为可选 role 的 OpenAIMessage", StringToOpenAIMessageNode);
    register_node!("openai_message_content_as_json", "OpenAIMessage内容转JSON", "消息", "将 OpenAIMessage 的 content 字符串解析为 JSON", OpenAIMessageContentAsJsonNode);
    register_node!("as_system_openai_message", "字符串转 OpenAIMessage", "消息", "兼容旧节点类型 ID：将字符串封装为可选 role 的 OpenAIMessage，默认 role=system", StringToOpenAIMessageNode);
    register_node!("preview_string", "Preview String", "工具", "在节点卡片内预览输入字符串", PreviewStringNode);
    register_node!("string_data", "String Data", "数据", "字符串数据源，通过UI输入框提供字符串", StringDataNode);
    register_node!("string_is_not_empty", "字符串非空判断", "工具", "判断字符串是否非空，可选 trim_before_check 决定是否先 trim 再判断", StringIsNotEmptyNode);
    register_node!("current_time", "当前时间", "数据", "输出当前本地时间字符串，无需输入", CurrentTimeNode);
    register_node!("preview_message_list", "Preview OpenAIMessage List", "工具", "在节点卡片内预览 OpenAIMessage 列表", PreviewMessageListNode);
    register_node!("message_list_data", "OpenAIMessage List Data", "数据", "OpenAIMessage 列表数据源，通过 UI 容器编辑器提供列表数据", MessageListDataNode);
    register_node!("qq_message_list_data", "QQMessageList Data", "数据", "QQ消息列表数据源，通过UI容器编辑器提供QQMessageList", QQMessageListDataNode);
    register_node!("string_to_plain_text", "字符串转QQ纯文本", "消息", "将字符串转换为 QQ 消息中的纯文本（PlainText）消息段", StringToPlainTextNode);
    register_node!("at_qq_target_message", "构造QQAt消息", "消息", "输入 QQ 目标 id 字符串，输出 @ 目标的 QQ 消息段", AtQQTargetMessageNode);
    register_node!("json_to_qq_message_vec", "JSON转QQMessage列表", "消息", "将 LLM 输出的 QQ 消息 JSON 二维数组转换为 Vec<Vec<QQMessage>>", JsonToQQMessageVecNode);
    register_node!("tool_result", "Tool 结果消息", "AI", "将工具执行结果封装为 role=tool 的 OpenAIMessage，供 agentic loop 回写对话列表", ToolResultNode);
    register_node!("redis", "Redis连接", "数据库", "构建Redis连接配置", RedisNode);
    register_node!("mysql", "MySQL连接", "数据库", "构建MySQL连接配置", MySqlNode);
    register_node!("message_mysql_persistence", "消息MySQL持久化", "消息存储", "将消息事件持久化到MySQL数据库", MessageMySQLPersistenceNode);
    register_node!("qq_message_list_mysql_persistence", "QQMessage列表MySQL持久化", "消息存储", "将Vec<QQMessage>及调用方提供的元数据持久化到MySQL数据库", QQMessageListMySQLPersistenceNode);
    register_node!("message_mysql_get_user_history", "获取QQ号消息历史", "消息存储", "根据 sender_id 读取最近消息历史，可选限定某个群", MessageMySQLGetUserHistoryNode);
    register_node!("message_mysql_get_group_history", "获取QQ群聊消息历史", "消息存储", "根据 group_id 读取最近消息历史", MessageMySQLGetGroupHistoryNode);
    register_node!("message_cache", "消息缓存", "消息存储", "缓存消息事件到内存或Redis", MessageCacheNode);
    register_node!("openai_message_session_cache_provider", "OpenAIMessage 会话暂存器", "消息存储", "创建单次节点图运行期的 OpenAIMessage 会话暂存引用，支持 Redis 或内存回退", OpenAIMessageSessionCacheProviderNode);
    register_node!("openai_message_session_cache", "OpenAIMessage 会话暂存", "消息存储", "根据缓存 Ref、sender_id 和消息列表向当前运行期会话历史追加 Vec<OpenAIMessage>", OpenAIMessageSessionCacheNode);
    register_node!("openai_message_session_cache_get", "获取 OpenAIMessage 历史", "消息存储", "根据 OpenAIMessage 会话缓存 Ref 和 sender_id 读取当前运行期累计的 Vec<OpenAIMessage>", OpenAIMessageSessionCacheGetNode);
    register_node!("openai_message_session_cache_set", "覆写 OpenAIMessage 历史", "消息存储", "根据 OpenAIMessage 会话缓存 Ref、sender_id 和消息列表覆写当前运行期累计的 Vec<OpenAIMessage>", OpenAIMessageSessionCacheSetNode);
    register_node!("openai_message_session_cache_clear", "清空 OpenAIMessage 历史", "消息存储", "根据 OpenAIMessage 会话缓存 Ref 和 sender_id 清空当前运行期累计的历史消息", OpenAIMessageSessionCacheClearNode);
    register_node!("session_state_provider", "运行时会话状态提供器", "消息存储", "创建跨事件共享的运行时会话状态引用", SessionStateProviderNode);
    register_node!("session_state_get", "读取会话状态", "消息存储", "读取 sender_id 当前是否处于会话中以及附加状态", SessionStateGetNode);
    register_node!("session_state_clear", "清除会话状态", "消息存储", "清除 sender_id 当前会话状态", SessionStateClearNode);
    register_node!("session_state_try_claim", "尝试占用会话", "消息存储", "原子检查并占用 sender_id 会话状态", SessionStateTryClaimNode);
    register_node!("session_state_release", "释放会话占用", "消息存储", "释放 sender_id 当前持有的会话占用", SessionStateReleaseNode);
    register_node!("qq_message_json_output_system_prompt_provider", "QQ消息JSON输出格式Prompt", "消息", "输出固定的 system prompt，要求 LLM 只返回 QQ 消息二维 JSON 数组", QQMessageJsonOutputSystemPromptProviderNode);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::json_to_data_value;
    use crate::{DataType, DataValue};

    #[test]
    fn parse_message_list_inline_value() {
        let json = serde_json::json!([
            {"role": "user", "content": "hi"},
            {"role": "ASSISTANT", "content": "hello"},
            {"role": "weird", "content": null}
        ]);

        let val = json_to_data_value(&json, &DataType::Vec(Box::new(DataType::OpenAIMessage)))
            .expect("should parse Vec<OpenAIMessage>");

        match val {
            DataValue::Vec(_, list) => {
                assert_eq!(list.len(), 3);
                match &list[0] {
                    DataValue::OpenAIMessage(m) => {
                        assert_eq!(zihuan_llm_types::role_to_str(&m.role), "user");
                        assert_eq!(m.content.as_deref(), Some("hi"));
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
                match &list[1] {
                    DataValue::OpenAIMessage(m) => {
                        assert_eq!(zihuan_llm_types::role_to_str(&m.role), "assistant");
                        assert_eq!(m.content.as_deref(), Some("hello"));
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
                match &list[2] {
                    DataValue::OpenAIMessage(m) => {
                        // Unknown role falls back to user
                        assert_eq!(zihuan_llm_types::role_to_str(&m.role), "user");
                        assert_eq!(m.content, None);
                    }
                    _ => panic!("expected OpenAIMessage"),
                }
            }
            _ => panic!("unexpected DataValue variant"),
        }
    }

    #[test]
    fn parse_any_inline_value() {
        let val = json_to_data_value(&serde_json::json!(123), &DataType::Any)
            .expect("should parse Any integer");

        match val {
            DataValue::Integer(value) => assert_eq!(value, 123),
            _ => panic!("unexpected DataValue variant"),
        }
    }
}
