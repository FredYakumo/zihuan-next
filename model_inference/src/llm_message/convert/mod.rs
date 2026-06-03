pub mod ims_message;
pub mod message_record;
pub mod openai_chat_completions;
pub mod openai_chat_completions_tencent_multimodal_compat;
pub mod openai_responses;
pub mod openai_responses_image_url_object_compat;
pub mod openai_responses_message_compat;

use zihuan_core::llm::{LLMMessage, LLMMessagePart};

pub use ims_message::{event_to_llm_message, qq_messages_to_llm_message};
pub use message_record::{llm_message_to_message_record, message_record_to_llm_message};
pub use openai_chat_completions::{
    build_chat_completions_request_body, parse_chat_completions_response,
    parse_chat_completions_sse_response, parse_chat_completions_sse_stream_response,
};
pub use openai_chat_completions_tencent_multimodal_compat::
    build_tencent_multimodal_chat_completions_request_body;
pub use openai_responses_image_url_object_compat::{
    build_responses_image_url_object_compat_request_body,
    parse_responses_image_url_object_compat_response,
    parse_responses_image_url_object_compat_sse_response,
    parse_responses_image_url_object_compat_sse_stream_response,
};
pub use openai_responses_message_compat::{
    build_responses_message_compat_request_body, parse_responses_message_compat_response,
    parse_responses_message_compat_sse_response,
    parse_responses_message_compat_sse_stream_response,
};
pub use openai_responses::{
    build_responses_request_body, parse_responses_response, parse_responses_sse_response,
    parse_responses_sse_stream_response,
};

pub fn has_multimodal_messages(messages: &[LLMMessage]) -> bool {
    messages.iter().any(|msg| {
        msg.parts.iter().any(|part| {
            matches!(part, LLMMessagePart::Image { .. } | LLMMessagePart::Video { .. })
        })
    })
}
