use crate::model_inference::model_config::{
    LlmApiStyle, LlmServiceConfig, ModelRefSpec, ModelWeightStyle, DEFAULT_CONTEXT_LENGTH,
};

/// Purpose: Verify a missing or null `context_length` falls back to the default window.
/// TestData: One config without `context_length` and one with `context_length: null`.
#[test]
fn context_length_defaults_for_missing_and_null_values() {
    let missing: LlmServiceConfig = serde_json::from_value(serde_json::json!({
        "model_name": "model",
        "api_endpoint": "http://127.0.0.1"
    }))
    .unwrap();
    let null_value: LlmServiceConfig = serde_json::from_value(serde_json::json!({
        "model_name": "model",
        "api_endpoint": "http://127.0.0.1",
        "context_length": null
    }))
    .unwrap();

    assert_eq!(missing.context_length, DEFAULT_CONTEXT_LENGTH);
    assert_eq!(null_value.context_length, DEFAULT_CONTEXT_LENGTH);
}

/// Purpose: Verify legacy `chat_llm` specs still dispatch to local/API variants by `api_style`.
/// TestData: A `candle_gguf` spec, a plain API spec, and a `mock` spec.
#[test]
fn legacy_chat_llm_specs_dispatch_by_api_style() {
    let gguf: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "chat_llm",
        "llm": { "model_name": "m", "api_endpoint": "", "api_style": "candle_gguf" }
    }))
    .unwrap();
    assert!(matches!(gguf, ModelRefSpec::ChatLlmLocal { .. }));

    let api: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "chat_llm",
        "llm": { "model_name": "m", "api_endpoint": "http://127.0.0.1" }
    }))
    .unwrap();
    assert!(matches!(api, ModelRefSpec::ChatLlmApi { .. }));

    let mock: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "chat_llm",
        "llm": { "model_name": "m", "api_endpoint": "recordings/x", "api_style": "mock" }
    }))
    .unwrap();
    match mock {
        ModelRefSpec::ChatLlmLocal { llm } => {
            assert_eq!(llm.weight_style, Some(ModelWeightStyle::Mock));
            assert_eq!(llm.api_endpoint, "recordings/x");
        }
        _ => panic!("expected chat_llm_local"),
    }
}

/// Purpose: Verify legacy `type: chat_llm` specs also accept the llm fields inline.
/// TestData: A `candle_hf` spec whose `model_name` / `api_endpoint` sit next to `type`.
#[test]
fn legacy_chat_llm_accepts_inline_llm_fields() {
    let spec: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "chat_llm",
        "model_name": "m",
        "api_endpoint": "weights/x",
        "api_style": "candle_hf"
    }))
    .unwrap();
    match spec {
        ModelRefSpec::ChatLlmLocal { llm } => {
            assert_eq!(llm.weight_style, Some(ModelWeightStyle::Hf));
            assert!(matches!(llm.api_style, LlmApiStyle::OpenAiChatCompletions));
        }
        other => panic!("expected chat_llm_local, got {other:?}"),
    }
}

/// Purpose: Verify the even older bare-`LlmServiceConfig` encoding still loads as an API chat ref.
/// TestData: A config object without any `type` tag.
#[test]
fn legacy_bare_llm_config_is_treated_as_chat_llm_api() {
    let spec: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "model_name": "m",
        "api_endpoint": "http://127.0.0.1"
    }))
    .unwrap();
    assert!(matches!(spec, ModelRefSpec::ChatLlmApi { .. }));
}

/// Purpose: Verify tagged chat specs deserialize from a JSON string (the admin request body path).
/// TestData: A `chat_llm_api` spec and a `chat_llm_local` spec, both read with `from_str`.
#[test]
fn tagged_chat_llm_specs_deserialize_from_json_str() {
    let api: ModelRefSpec = serde_json::from_str(
        r#"{"type":"chat_llm_api","llm":{"model_name":"zihuan-0.15",
            "api_endpoint":"http://127.0.0.1L:19951"}}"#,
    )
    .unwrap();
    match api {
        ModelRefSpec::ChatLlmApi { llm } => {
            assert_eq!(llm.model_name, "zihuan-0.15");
            assert_eq!(llm.timeout_secs, 30);
        }
        other => panic!("expected chat_llm_api, got {other:?}"),
    }

    let local: ModelRefSpec = serde_json::from_str(
        r#"{"type":"chat_llm_local","llm":{"model_name":"qwen",
            "api_endpoint":"weights/qwen","weight_style":"gguf"}}"#,
    )
    .unwrap();
    match local {
        ModelRefSpec::ChatLlmLocal { llm } => {
            assert_eq!(llm.weight_style, Some(ModelWeightStyle::Gguf));
        }
        other => panic!("expected chat_llm_local, got {other:?}"),
    }
}

/// Purpose: Verify tagged embedding specs deserialize and apply the documented defaults.
/// TestData: A `text_embedding_local` spec and a `text_embedding_api` spec without optional fields.
#[test]
fn tagged_text_embedding_specs_apply_defaults() {
    let local: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "text_embedding_local",
        "model_name": "qwen-3-embedding"
    }))
    .unwrap();
    assert!(matches!(local, ModelRefSpec::TextEmbeddingLocal { .. }));

    let api: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "text_embedding_api",
        "model_name": "qwen-3-embedding",
        "api_endpoint": "http://127.0.0.1:19951"
    }))
    .unwrap();
    match api {
        ModelRefSpec::TextEmbeddingApi {
            model_name,
            api_key,
            timeout_secs,
            retry_count,
            ..
        } => {
            assert_eq!(model_name, "qwen-3-embedding");
            assert_eq!(api_key, None);
            assert_eq!(timeout_secs, 30);
            assert_eq!(retry_count, 2);
        }
        other => panic!("expected text_embedding_api, got {other:?}"),
    }
}

/// Purpose: Verify the tagged encoding survives the admin read-then-write round trip.
/// TestData: A `chat_llm_api` spec serialized with `to_value` and parsed back.
#[test]
fn tagged_specs_round_trip_through_json() {
    let spec: ModelRefSpec = serde_json::from_value(serde_json::json!({
        "type": "chat_llm_api",
        "llm": { "model_name": "m", "api_endpoint": "http://127.0.0.1" }
    }))
    .unwrap();
    let stored = serde_json::to_value(&spec).unwrap();
    assert_eq!(stored["type"].as_str(), Some("chat_llm_api"));
    assert_eq!(stored["llm"]["model_name"].as_str(), Some("m"));

    let reparsed: ModelRefSpec = serde_json::from_value(stored).unwrap();
    assert!(matches!(reparsed, ModelRefSpec::ChatLlmApi { .. }));
}

/// Purpose: Verify an unrecognized `type` tag is rejected instead of being treated as legacy.
/// TestData: A spec tagged `nope`.
#[test]
fn unknown_model_ref_type_is_rejected() {
    let err =
        serde_json::from_value::<ModelRefSpec>(serde_json::json!({ "type": "nope" })).unwrap_err();
    assert!(err.to_string().contains("unknown model ref type"), "unexpected error: {err}");
}
