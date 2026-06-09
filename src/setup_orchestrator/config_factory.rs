use std::collections::HashMap;

use crate::api::config::now_rfc3339;
use crate::setup_orchestrator::{LlmSetupConfig, ImsBotAdapterSetupConfig};
use crate::system_config;
use ims_bot_adapter::BotAdapterConnection;
use model_inference::system_config::{
    AgentConfig, AgentType, HttpStreamAgentConfig, LlmRefConfig, LlmServiceConfig, ModelRefSpec,
};
use storage_handler::{
    ConnectionConfig, ConnectionKind, RedisConnection, RustfsConnection, SqliteConnection, WeaviateConnection,
    WebSearchEngineConnection,
};
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_core::weaviate::WeaviateCollectionSchema;

pub async fn create_chat_assistant_stack(llm_config: &LlmSetupConfig) -> Result<(), String> {
    let llm_ref = build_llm_ref(llm_config, "setup-default-llm", "Default LLM");
    save_llm_ref(llm_ref)?;

    let agent = build_http_stream_agent(
        "setup-default-agent",
        "Chat Assistant",
        Some("setup-default-llm".to_string()),
        None,
        None,
        None,
        "setup-default-sqlite".to_string(),
    );
    save_agent(agent)?;

    Ok(())
}

pub async fn create_qq_bot_stack(
    llm_config: &LlmSetupConfig,
    ims_config: &ImsBotAdapterSetupConfig,
    napcat_native_path: Option<&str>,
) -> Result<(), String> {
    let llm_ref = build_llm_ref(llm_config, "setup-default-llm", "Default LLM");
    save_llm_ref(llm_ref)?;

    let embedding_ref = build_embedding_ref("setup-default-embedding", "Default Embedding");
    save_llm_ref(embedding_ref)?;

    let redis = build_connection(
        "setup-default-redis",
        "Redis",
        ConnectionKind::Redis(RedisConnection {
            url: "redis://127.0.0.1:6379".to_string(),
            username: None,
            password: None,
        }),
    );
    save_connection(redis)?;

    let weaviate_image = build_connection(
        "setup-default-weaviate-image",
        "Weaviate Image",
        ConnectionKind::Weaviate(WeaviateConnection {
            base_url: "http://127.0.0.1:8080".to_string(),
            class_name: "ImageSemantic".to_string(),
            username: None,
            password: None,
            api_key: None,
            collection_schema: WeaviateCollectionSchema::ImageSemantic,
        }),
    );
    save_connection(weaviate_image)?;

    let weaviate_memory = build_connection(
        "setup-default-weaviate-memory",
        "Weaviate Memory",
        ConnectionKind::Weaviate(WeaviateConnection {
            base_url: "http://127.0.0.1:8080".to_string(),
            class_name: "AgentMemory".to_string(),
            username: None,
            password: None,
            api_key: None,
            collection_schema: WeaviateCollectionSchema::AgentMemory,
        }),
    );
    save_connection(weaviate_memory)?;

    let rustfs = build_connection(
        "setup-default-rustfs",
        "RustFS",
        ConnectionKind::Rustfs(RustfsConnection {
            endpoint: "http://127.0.0.1:9000".to_string(),
            bucket: "zihuan".to_string(),
            region: "us-east-1".to_string(),
            access_key: "minioadmin".to_string(),
            secret_key: "minioadmin".to_string(),
            public_base_url: None,
            path_style: true,
        }),
    );
    save_connection(rustfs)?;

    let bot_adapter = build_connection(
        "setup-default-bot-adapter",
        "QQ Bot Adapter (NapCat)",
        ConnectionKind::BotAdapter(
            serde_json::to_value(BotAdapterConnection {
                bot_server_url: ims_config.ws_url.clone(),
                adapter_server_url: None,
                bot_server_token: ims_config.token.clone(),
                qq_id: ims_config.qq_id.clone(),
                napcat_install_path: napcat_native_path.map(|s| s.to_string()),
            })
            .unwrap_or(serde_json::Value::Null),
        ),
    );
    save_connection(bot_adapter)?;

    let web_search = build_connection(
        "setup-default-web-search",
        "Web Search",
        ConnectionKind::WebSearchEngine(WebSearchEngineConnection {
            provider: "tavily".to_string(),
            api_token: None,
            timeout_secs: 30,
        }),
    );
    save_connection(web_search)?;

    let sqlite = build_connection(
        "setup-default-sqlite",
        "SQLite Task DB",
        ConnectionKind::Sqlite(SqliteConnection {
            path: "zihuan_data.db".to_string(),
        }),
    );
    save_connection(sqlite)?;

    let agent = build_qq_chat_agent();
    save_agent(agent)?;

    Ok(())
}

pub async fn create_butler_stack(llm_config: &LlmSetupConfig) -> Result<(), String> {
    let llm_ref = build_llm_ref(llm_config, "setup-default-llm", "Default LLM");
    save_llm_ref(llm_ref)?;

    let embedding_ref = build_embedding_ref("setup-default-embedding", "Default Embedding");
    save_llm_ref(embedding_ref)?;

    let redis = build_connection(
        "setup-default-redis",
        "Redis",
        ConnectionKind::Redis(RedisConnection {
            url: "redis://127.0.0.1:6379".to_string(),
            username: None,
            password: None,
        }),
    );
    save_connection(redis)?;

    let weaviate_memory = build_connection(
        "setup-default-weaviate-memory",
        "Weaviate Memory",
        ConnectionKind::Weaviate(WeaviateConnection {
            base_url: "http://127.0.0.1:8080".to_string(),
            class_name: "AgentMemory".to_string(),
            username: None,
            password: None,
            api_key: None,
            collection_schema: WeaviateCollectionSchema::AgentMemory,
        }),
    );
    save_connection(weaviate_memory)?;

    let rustfs = build_connection(
        "setup-default-rustfs",
        "RustFS",
        ConnectionKind::Rustfs(RustfsConnection {
            endpoint: "http://127.0.0.1:9000".to_string(),
            bucket: "zihuan".to_string(),
            region: "us-east-1".to_string(),
            access_key: "minioadmin".to_string(),
            secret_key: "minioadmin".to_string(),
            public_base_url: None,
            path_style: true,
        }),
    );
    save_connection(rustfs)?;

    let web_search = build_connection(
        "setup-default-web-search",
        "Web Search",
        ConnectionKind::WebSearchEngine(WebSearchEngineConnection {
            provider: "tavily".to_string(),
            api_token: None,
            timeout_secs: 30,
        }),
    );
    save_connection(web_search)?;

    let sqlite = build_connection(
        "setup-default-sqlite",
        "SQLite Task DB",
        ConnectionKind::Sqlite(SqliteConnection {
            path: "zihuan_data.db".to_string(),
        }),
    );
    save_connection(sqlite)?;

    let agent = build_http_stream_agent(
        "setup-default-agent",
        "AI Butler",
        Some("setup-default-llm".to_string()),
        Some("setup-default-embedding".to_string()),
        Some("setup-default-web-search".to_string()),
        Some("setup-default-weaviate-memory".to_string()),
        "setup-default-sqlite".to_string(),
    );
    save_agent(agent)?;

    Ok(())
}

fn build_llm_ref(config: &LlmSetupConfig, id: &str, name: &str) -> LlmRefConfig {
    let model = if config.mode == "local" {
        ModelRefSpec::TextEmbeddingLocal {
            model_name: config.model_name.clone(),
        }
    } else {
        ModelRefSpec::ChatLlm {
            llm: LlmServiceConfig {
                model_name: config.model_name.clone(),
                api_endpoint: config.api_endpoint.clone(),
                api_key: config.api_key.clone(),
                api_style: parse_api_style(&config.api_style),
                stream: true,
                supports_multimodal_input: false,
                include_reasoning_content: false,
                thinking_type: None,
                reasoning_effort: None,
                timeout_secs: 120,
                retry_count: 2,
            },
        }
    };

    LlmRefConfig {
        id: id.to_string(),
        config_id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        model,
        updated_at: now_rfc3339(),
    }
}

fn build_embedding_ref(id: &str, name: &str) -> LlmRefConfig {
    LlmRefConfig {
        id: id.to_string(),
        config_id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        model: ModelRefSpec::TextEmbeddingLocal {
            model_name: "bge-small-zh-v1.5".to_string(),
        },
        updated_at: now_rfc3339(),
    }
}

fn build_connection(id: &str, name: &str, kind: ConnectionKind) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_string(),
        config_id: id.to_string(),
        name: name.to_string(),
        enabled: true,
        kind,
        updated_at: now_rfc3339(),
    }
}

fn build_http_stream_agent(
    id: &str,
    name: &str,
    llm_ref_id: Option<String>,
    embedding_model_ref_id: Option<String>,
    web_search_engine_connection_id: Option<String>,
    weaviate_memory_connection_id: Option<String>,
    task_db_connection_id: String,
) -> AgentConfig {
    AgentConfig {
        id: id.to_string(),
        config_id: id.to_string(),
        name: name.to_string(),
        agent_type: AgentType::HttpStream(HttpStreamAgentConfig {
            bind: "127.0.0.1:18080".to_string(),
            api_key: None,
            llm_ref_id,
            embedding_model_ref_id,
            web_search_engine_connection_id,
            weaviate_memory_connection_id,
            task_db_connection_id,
            default_tools_enabled: default_http_stream_tools(),
        }),
        enabled: true,
        auto_start: false,
        is_default: false,
        updated_at: now_rfc3339(),
        tools: vec![],
    }
}

fn build_qq_chat_agent() -> AgentConfig {
    let mut default_tools = HashMap::new();
    for tool in [
        "web_search",
        "get_agent_public_info",
        "get_function_list",
        "get_recent_group_messages",
        "get_recent_user_messages",
        "search_similar_images",
        "image_understand",
        "list_available_memory_keys",
        "search_memory_content",
        "remember_content",
        "remove_memory",
    ] {
        default_tools.insert(tool.to_string(), true);
    }

    AgentConfig {
        id: "setup-default-agent".to_string(),
        config_id: "setup-default-agent".to_string(),
        name: "QQ Chat Bot".to_string(),
        agent_type: AgentType::QqChat(QqChatAgentConfig {
            ims_bot_adapter_connection_id: "setup-default-bot-adapter".to_string(),
            rustfs_connection_id: Some("setup-default-rustfs".to_string()),
            bot_name: "ZihuanBot".to_string(),
            system_prompt: None,
            llm_ref_id: Some("setup-default-llm".to_string()),
            image_understand_llm_ref_id: None,
            math_programming_llm_ref_id: None,
            natural_language_reply_llm_ref_id: None,
            natural_language_reply_system_prompt: None,
            embedding_model_ref_id: Some("setup-default-embedding".to_string()),
            tokenizer_connection_id: None,
            web_search_engine_connection_id: "setup-default-web-search".to_string(),
            rdb_id: Some("setup-default-sqlite".to_string()),
            embedding: None,
            mysql_connection_id: None,
            task_db_connection_id: None,
            weaviate_image_connection_id: Some("setup-default-weaviate-image".to_string()),
            weaviate_memory_connection_id: Some("setup-default-weaviate-memory".to_string()),
            max_message_length: 500,
            compact_context_length: 0,
            max_steer_count: 4,
            default_tools_enabled: default_tools,
            emotion_dimensions: vec![],
            event_handler_threads: None,
        }),
        enabled: true,
        auto_start: false,
        is_default: false,
        updated_at: now_rfc3339(),
        tools: vec![],
    }
}

fn parse_api_style(value: &str) -> model_inference::system_config::LlmApiStyle {
    match value {
        "candle" => model_inference::system_config::LlmApiStyle::Candle,
        "open_ai_responses" => model_inference::system_config::LlmApiStyle::OpenAiResponses,
        "open_ai_responses_message_compat" => model_inference::system_config::LlmApiStyle::OpenAiResponsesMessageCompat,
        "open_ai_responses_image_url_object_compat" => {
            model_inference::system_config::LlmApiStyle::OpenAiResponsesImageUrlObjectCompat
        }
        "open_ai_chat_completions_tencent_multimodal_compat" => {
            model_inference::system_config::LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
        }
        _ => model_inference::system_config::LlmApiStyle::OpenAiChatCompletions,
    }
}

fn default_http_stream_tools() -> HashMap<String, bool> {
    [
        ("web_search".to_string(), true),
        ("list_available_memory_keys".to_string(), true),
        ("search_memory_content".to_string(), true),
        ("remember_content".to_string(), true),
    ]
    .into_iter()
    .collect()
}

fn save_llm_ref(llm_ref: LlmRefConfig) -> Result<(), String> {
    let mut llm_refs = system_config::load_llm_refs().map_err(|e| e.to_string())?;
    llm_refs.retain(|r| r.id != llm_ref.id);
    llm_refs.push(llm_ref);
    system_config::save_llm_refs(llm_refs).map_err(|e| e.to_string())
}

fn save_agent(agent: AgentConfig) -> Result<(), String> {
    let mut agents = system_config::load_agents().map_err(|e| e.to_string())?;
    agents.retain(|a| a.id != agent.id);
    agents.push(agent);
    system_config::save_agents(agents).map_err(|e| e.to_string())
}

fn save_connection(connection: ConnectionConfig) -> Result<(), String> {
    let mut connections = system_config::load_connections().map_err(|e| e.to_string())?;
    connections.retain(|c| c.id != connection.id);
    connections.push(connection);
    system_config::save_connections(connections).map_err(|e| e.to_string())
}
