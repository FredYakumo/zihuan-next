pub mod and_then;
pub mod array_get;
pub mod at_qq_target_message;
pub mod binary_to_image_content_part;
pub mod boolean_branch;
pub mod boolean_not;
pub mod build_multimodal_user_message;
pub mod concat_vec;
pub mod conditional;
pub mod conditional_router;
pub mod current_time;
pub mod format_string;
pub mod function;
pub mod function_inputs;
pub mod function_outputs;
pub mod graph_inputs;
pub mod graph_outputs;
pub mod join_string;
pub mod json_extract;
pub mod json_parser;
pub mod json_to_qq_message_vec;
pub mod message_content;
pub mod message_list_data;
pub mod openai_message_content_as_json;
pub mod openai_message_session_cache;
pub mod openai_message_session_cache_get;
pub mod openai_message_session_cache_set;
pub mod openai_message_to_string;
pub mod preview_message_list;
pub mod preview_qq_message_list;
pub mod preview_string;
pub mod push_back_vec;
pub mod qq_message_json_output_system_prompt_provider;
pub mod qq_message_list_data;
pub mod qq_message_to_image;
pub mod session_state_clear;
pub mod session_state_get;
pub mod session_state_release;
pub mod session_state_try_claim;
pub mod set_variable;
pub mod stack;
pub mod string_data;
pub mod string_is_not_empty;
pub mod string_to_image_content_part;
pub mod string_to_openai_message;
pub mod string_to_plain_text;
pub mod switch;
pub mod tool_result_node;

pub mod openai_message_session_cache_clear {
    use crate::data_value::OpenAIMessageSessionCacheRef;
    use crate::{node_input, node_output, DataType, DataValue, Node, Port};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::task::block_in_place;
    use zihuan_core::error::Result;

    pub struct OpenAIMessageSessionCacheClearNode {
        id: String,
        name: String,
    }

    impl OpenAIMessageSessionCacheClearNode {
        pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                name: name.into(),
            }
        }
    }

    impl Node for OpenAIMessageSessionCacheClearNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> Option<&str> {
            Some("根据缓存 Ref 和 sender_id 清空当前运行期累计的 Vec<OpenAIMessage>")
        }

        node_input![
            port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "OpenAIMessage 会话暂存器输出的缓存引用" },
            port! { name = "sender_id", ty = String, desc = "要清空历史消息的 sender_id" },
        ];

        node_output![
            port! { name = "cleared", ty = Boolean, desc = "是否成功清空至少一条历史消息" },
        ];

        fn execute(
            &mut self,
            inputs: HashMap<String, DataValue>,
        ) -> Result<HashMap<String, DataValue>> {
            self.validate_inputs(&inputs)?;

            let cache_ref: Arc<OpenAIMessageSessionCacheRef> = inputs
                .get("cache_ref")
                .and_then(|value| match value {
                    DataValue::OpenAIMessageSessionCacheRef(cache_ref) => Some(cache_ref.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    zihuan_core::error::Error::InvalidNodeInput("cache_ref is required".to_string())
                })?;

            let sender_id = inputs
                .get("sender_id")
                .and_then(|value| match value {
                    DataValue::String(sender_id) => Some(sender_id.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    zihuan_core::error::Error::InvalidNodeInput("sender_id is required".to_string())
                })?;

            let clear_messages = async move { cache_ref.clear_messages(&sender_id).await };

            let cleared = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(clear_messages))
            } else {
                tokio::runtime::Runtime::new()?.block_on(clear_messages)
            }?;

            let mut outputs = HashMap::new();
            outputs.insert("cleared".to_string(), DataValue::Boolean(cleared));

            self.validate_outputs(&outputs)?;
            Ok(outputs)
        }
    }
}

pub use and_then::AndThenNode;
pub use array_get::ArrayGetNode;
pub use at_qq_target_message::AtQQTargetMessageNode;
pub use binary_to_image_content_part::BinaryToImageContentPartNode;
pub use boolean_branch::BooleanBranchNode;
pub use boolean_not::BooleanNotNode;
pub use build_multimodal_user_message::BuildMultimodalUserMessageNode;
pub use concat_vec::ConcatVecNode;
pub use conditional::ConditionalNode;
pub use conditional_router::ConditionalRouterNode;
pub use current_time::CurrentTimeNode;
pub use format_string::FormatStringNode;
pub use function::FunctionNode;
pub use function_inputs::FunctionInputsNode;
pub use function_outputs::FunctionOutputsNode;
pub use graph_inputs::GraphInputsNode;
pub use graph_outputs::GraphOutputsNode;
pub use join_string::JoinStringNode;
pub use json_extract::JsonExtractNode;
pub use json_parser::JsonParserNode;
pub use json_to_qq_message_vec::JsonToQQMessageVecNode;
pub use message_content::MessageContentNode;
pub use message_list_data::MessageListDataNode;
pub use openai_message_content_as_json::OpenAIMessageContentAsJsonNode;
pub use openai_message_session_cache::OpenAIMessageSessionCacheNode;
pub use openai_message_session_cache_clear::OpenAIMessageSessionCacheClearNode;
pub use openai_message_session_cache_get::OpenAIMessageSessionCacheGetNode;
pub use openai_message_session_cache_set::OpenAIMessageSessionCacheSetNode;
pub use openai_message_to_string::OpenAIMessageToStringNode;
pub use preview_message_list::PreviewMessageListNode;
pub use preview_qq_message_list::PreviewQQMessageListNode;
pub use preview_string::PreviewStringNode;
pub use push_back_vec::PushBackVecNode;
pub use qq_message_json_output_system_prompt_provider::QQMessageJsonOutputSystemPromptProviderNode;
pub use qq_message_list_data::QQMessageListDataNode;
pub use qq_message_to_image::QQMessageToImageNode;
pub use session_state_clear::SessionStateClearNode;
pub use session_state_get::SessionStateGetNode;
pub use session_state_release::SessionStateReleaseNode;
pub use session_state_try_claim::SessionStateTryClaimNode;
pub use set_variable::SetVariableNode;
pub use stack::StackNode;
pub use string_data::{StringDataNode, STRING_DATA_CONTEXT};
pub use string_is_not_empty::StringIsNotEmptyNode;
pub use string_to_image_content_part::StringToImageContentPartNode;
pub use string_to_openai_message::StringToOpenAIMessageNode;
pub use string_to_plain_text::StringToPlainTextNode;
pub use switch::SwitchNode;
pub use tool_result_node::ToolResultNode;
