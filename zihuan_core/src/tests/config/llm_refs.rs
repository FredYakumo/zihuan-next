use serde_json::json;

use crate::config::llm_refs::LlmRefConfig;
use crate::model_inference::model_config::ModelRefSpec;

/// Purpose: Verify a stored `chat_llm_api` ref loads with its tagged `model` spec.
/// TestData: An `LlmRefConfig` payload whose model uses the `chat_llm_api` tag.
#[test]
fn tagged_chat_specs_load_into_llm_refs() {
    let llm_ref: LlmRefConfig = serde_json::from_value(json!({
        "config_id": "llm-ref-1",
        "name": "deepseek",
        "enabled": true,
        "model": {
            "type": "chat_llm_api",
            "llm": {
                "model_name": "zihuan-0.15",
                "api_endpoint": "http://127.0.0.1:19951"
            }
        }
    }))
    .unwrap();

    assert_eq!(llm_ref.canonical_config_id(), "llm-ref-1");
    match llm_ref.chat_llm() {
        Some(llm) => assert_eq!(llm.model_name, "zihuan-0.15"),
        None => panic!("expected a chat llm ref"),
    }
}

/// Purpose: Verify a stored `text_embedding_api` ref loads with its tagged spec and defaults.
/// TestData: An `LlmRefConfig` payload whose model is a tagged embedding API spec.
#[test]
fn tagged_embedding_specs_load_into_llm_refs() {
    let llm_ref: LlmRefConfig = serde_json::from_value(json!({
        "config_id": "llm-ref-2",
        "name": "embed",
        "model": {
            "type": "text_embedding_api",
            "model_name": "qwen-3-embedding",
            "api_endpoint": "http://127.0.0.1:19951"
        }
    }))
    .unwrap();

    assert!(llm_ref.local_embedding_model_name().is_err());
    let embedding = llm_ref.embedding_api_service_config().unwrap();
    assert_eq!(embedding.model_name, "qwen-3-embedding");
    assert_eq!(embedding.timeout_secs, 30);
    assert_eq!(embedding.retry_count, 2);
}

/// Purpose: Verify the tagged spec written by the admin view loads back unchanged.
/// TestData: A `text_embedding_api` ref serialized with `to_value` and parsed again.
#[test]
fn llm_refs_round_trip_through_json() {
    let stored: LlmRefConfig = serde_json::from_value(json!({
        "config_id": "llm-ref-3",
        "name": "embed",
        "enabled": true,
        "model": {
            "type": "text_embedding_api",
            "model_name": "bge-m3",
            "api_endpoint": "http://127.0.0.1:19951"
        }
    }))
    .unwrap();

    let reserialized = serde_json::to_value(&stored).unwrap();
    assert_eq!(reserialized["model"]["type"].as_str(), Some("text_embedding_api"));

    let reloaded: LlmRefConfig = serde_json::from_value(reserialized).unwrap();
    assert_eq!(reloaded.canonical_config_id(), "llm-ref-3");
    match reloaded.model {
        ModelRefSpec::TextEmbeddingApi { model_name, .. } => assert_eq!(model_name, "bge-m3"),
        other => panic!("expected text_embedding_api, got {other:?}"),
    }
}
