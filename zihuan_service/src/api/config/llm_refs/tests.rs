use zihuan_core::model_inference::model_config::ModelRefSpec;

use super::{CreateLlmRefRequest, UpdateLlmRefRequest};

/// Purpose: Verify an admin update request parses a tagged chat spec from the raw body.
/// TestData: An `UpdateLlmRefRequest` body carrying a `chat_llm_api` model spec.
#[test]
fn update_request_parses_tagged_chat_spec_from_slice() {
    let body = br#"{
        "name": "deepseek",
        "enabled": true,
        "model": {
            "type": "chat_llm_api",
            "llm": {
                "model_name": "zihuan-0.15",
                "api_endpoint": "http://127.0.0.1:19951"
            }
        }
    }"#;
    let request: UpdateLlmRefRequest = serde_json::from_slice(body).unwrap();

    assert_eq!(request.name, "deepseek");
    assert!(request.enabled);
    match request.model {
        ModelRefSpec::ChatLlmApi { llm } => {
            assert_eq!(llm.model_name, "zihuan-0.15");
            assert_eq!(llm.context_length, 32 * 1024);
        }
        other => panic!("expected chat_llm_api, got {other:?}"),
    }
}

/// Purpose: Verify an admin create request parses a tagged embedding spec from the raw body.
/// TestData: A `CreateLlmRefRequest` body carrying a `text_embedding_api` model spec.
#[test]
fn create_request_parses_tagged_embedding_spec_from_slice() {
    let body = br#"{
        "name": "embed",
        "model": {
            "type": "text_embedding_api",
            "model_name": "bge-m3",
            "api_endpoint": "http://127.0.0.1:19951"
        }
    }"#;
    let request: CreateLlmRefRequest = serde_json::from_slice(body).unwrap();

    assert_eq!(request.name, "embed");
    assert!(!request.enabled);
    match request.model {
        ModelRefSpec::TextEmbeddingApi {
            model_name, timeout_secs, retry_count, ..
        } => {
            assert_eq!(model_name, "bge-m3");
            assert_eq!(timeout_secs, 30);
            assert_eq!(retry_count, 2);
        }
        other => panic!("expected text_embedding_api, got {other:?}"),
    }
}
