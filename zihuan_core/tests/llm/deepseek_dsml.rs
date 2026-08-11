//! Integration tests for DeepSeek DSML tool-call compatibility.

use std::io::{Read, Write};
use std::net::TcpListener;

use serde_json::json;
use zihuan_core::inference::llm_message::convert::{
    parse_chat_completions_response, parse_chat_completions_sse_stream_response,
};
use zihuan_core::llm::MessagePart;

fn message_text(message: &zihuan_core::llm::LLMMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.as_str()),
            MessagePart::Image { .. } | MessagePart::Video { .. } => None,
        })
        .collect()
}

/// Purpose:
///     Verify that a complete DSML tool-call block is removed from assistant
///     content and converted into a structured function call with typed values.
///
/// Test Data:
///     - One assistant response with ordinary text before and after a DSML block.
///     - The DSML invocation has a string path, an integer line number, a
///       boolean flag, and a JSON object parameter.
#[test]
fn parse_chat_completions_response_converts_dsml_parameters_to_typed_tool_call() {
    let response = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "检查文件。<|DSML|tool_calls><|DSML|invoke name=\"read_file\"><|DSML|parameter name=\"path\" string=\"true\">C:/repo/src/lib.rs</|DSML|parameter><|DSML|parameter name=\"end_line\" string=\"false\">260</|DSML|parameter><|DSML|parameter name=\"follow\" string=\"false\">true</|DSML|parameter><|DSML|parameter name=\"options\" string=\"false\">{\"verbose\":true}</|DSML|parameter></|DSML|invoke></|DSML|tool_calls>完成。"
            }
        }]
    });

    let message = parse_chat_completions_response(&response).expect("parse response");
    assert_eq!(message_text(&message), "检查文件。完成。");
    assert_eq!(message.tool_calls.len(), 1);
    assert_eq!(message.tool_calls[0].function.name, "read_file");
    assert_eq!(message.tool_calls[0].function.arguments["path"], "C:/repo/src/lib.rs");
    assert_eq!(message.tool_calls[0].function.arguments["end_line"], 260);
    assert_eq!(message.tool_calls[0].function.arguments["follow"], true);
    assert_eq!(message.tool_calls[0].function.arguments["options"]["verbose"], true);
}

/// Purpose:
///     Verify that DSML calls merge with native OpenAI `tool_calls`, retain
///     multiple invocations, and avoid duplicate call identifiers.
///
/// Test Data:
///     - One native function call whose id collides with DSML's generated id.
///     - Two DSML invocations using the full-width DeepSeek delimiter variant.
#[test]
fn parse_chat_completions_response_merges_native_and_multiple_dsml_tool_calls() {
    let response = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "<｜DSML｜tool_calls><｜DSML｜invoke name=\"list_dir\"><｜DSML｜parameter name=\"path\" string=\"true\">src</｜DSML｜parameter></｜DSML｜invoke><｜DSML｜invoke name=\"grep\"><｜DSML｜parameter name=\"query\" string=\"true\">DSML</｜DSML｜parameter></｜DSML｜invoke></｜DSML｜tool_calls>",
                "tool_calls": [{
                    "id": "dsml_tool_call_0",
                    "type": "function",
                    "function": { "name": "native_tool", "arguments": "{}" }
                }]
            }
        }]
    });

    let message = parse_chat_completions_response(&response).expect("parse response");
    assert!(message.parts.is_empty());
    assert_eq!(message.tool_calls.len(), 3);
    assert_eq!(message.tool_calls[0].function.name, "native_tool");
    assert_eq!(message.tool_calls[1].function.name, "list_dir");
    assert_eq!(message.tool_calls[2].function.name, "grep");
    assert_ne!(message.tool_calls[0].id, message.tool_calls[1].id);
}

/// Purpose:
///     Verify that streaming SSE content split at DSML tag and parameter
///     boundaries is collected as a tool call without leaking protocol text to
///     the token channel.
///
/// Test Data:
///     - Four OpenAI-compatible SSE `delta.content` records delivered from a
///       local HTTP server, split through the opening tag, parameter tag, and
///       closing tags of one invocation.
#[test]
fn parse_chat_completions_sse_stream_response_hides_dsml_split_across_deltas() {
    let sse = concat!(
        "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"正在处理 <|DSML|tool\"}}]}\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"_calls><|DSML|invoke name=\\\"read_file\\\"><|DSML|parameter name=\\\"path\\\" string=\\\"true\\\">\"}}]}\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"C:/repo/AGENTS.md</|DSML|parameter></|DSML|invoke>\"}}]}\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"</|DSML|tool_calls>\"}}]}\n",
        "data: [DONE]\n"
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let response_body = sse.as_bytes().to_vec();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0; 1024];
        let _ = stream.read(&mut request);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        )
        .expect("write response headers");
        stream.write_all(&response_body).expect("write response body");
    });

    let runtime = tokio::runtime::Runtime::new().expect("create runtime");
    let response = runtime
        .block_on(reqwest::get(format!("http://{address}")))
        .expect("request SSE response");
    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel();
    let message = runtime.block_on(parse_chat_completions_sse_stream_response(response, token_tx));
    server.join().expect("join test server");

    let mut emitted_tokens = Vec::new();
    while let Ok(token) = token_rx.try_recv() {
        emitted_tokens.push(token.as_str().to_string());
    }

    assert_eq!(message_text(&message), "正在处理 ");
    assert_eq!(message.tool_calls.len(), 1);
    assert_eq!(message.tool_calls[0].function.name, "read_file");
    assert_eq!(message.tool_calls[0].function.arguments["path"], "C:/repo/AGENTS.md");
    assert_eq!(emitted_tokens, vec!["正在处理 "]);
}

/// Purpose:
///     Verify that normal text and malformed DSML-like content remain visible
///     instead of being silently discarded.
///
/// Test Data:
///     - A plain message containing a DSML-looking comparison.
///     - An unclosed DSML tool-call block with one parameter.
#[test]
fn parse_chat_completions_response_preserves_plain_and_malformed_dsml_text() {
    let response = json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "示例 <|DSML| is not a call. <|DSML|tool_calls><|DSML|invoke name=\"read_file\"><|DSML|parameter name=\"path\" string=\"true\">src/lib.rs"
            }
        }]
    });

    let message = parse_chat_completions_response(&response).expect("parse response");
    assert_eq!(
        message_text(&message),
        response["choices"][0]["message"]["content"].as_str().expect("response content")
    );
    assert!(message.tool_calls.is_empty());
}
