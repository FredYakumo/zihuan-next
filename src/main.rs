mod bot_adapter;
mod util;
mod llm;
mod config;
mod error;
mod node;

use log::{info, error, warn};
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;
use clap::Parser;
use std::sync::Arc;
use std::time::Duration;

use bot_adapter::adapter::{BotAdapter, BotAdapterConfig};
use config::{load_config, build_redis_url, build_mysql_url};
use llm::llm_api::LLMAPI;
use llm::function_tools::{MathTool, NaturalLanguageReplyTool, CodeWriterTool};
use crate::llm::agent::brain::BrainAgent;



lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next_aibot", "logs");
}


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short = 'l', long = "login-qq", help = "登录的QQ号（必填）")]
    qq_id: String,
}

#[tokio::main]
async fn main() {
    // Initialize logging using LogUtil
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");

    // Parse command line arguments
    let args = Args::parse();

    info!("zihuan_next_aibot-800b starting...");
    info!("登录的QQ号: {}", args.qq_id);

    // Load configuration from config.yaml, fallback to environment variables
    let config = load_config();

    // Build REDIS_URL from config
    let redis_url = build_redis_url(&config);

    if redis_url.is_some() {
        info!("Redis URL configured from config.yaml or environment");
    } else {
        warn!("No REDIS_URL or REDIS_HOST/PORT found in config.yaml; Redis will not be used.");
    }

    // Build DATABASE_URL (MySQL) from config
    let database_url = build_mysql_url(&config);

    if database_url.is_some() {
        info!("MySQL Database URL configured from config.yaml or environment");
    } else {
        warn!("No DATABASE_URL or MYSQL_* found in config.yaml; MySQL persistence will not be used.");
    }

    // Initialize LLM and function tools
    let agent_llm = if let (Some(api_endpoint), Some(model_name)) = (
        config.agent_model_api.clone(),
        config.agent_model_name.clone(),
    ) {
        let api_key = config.agent_model_api_key.clone();
        info!("Initializing Agent LLM: {} with endpoint: {}", model_name, api_endpoint);
        Arc::new(LLMAPI::new(
            model_name,
            api_endpoint,
            api_key,
            Duration::from_secs(30),
        )) as Arc<dyn llm::LLMBase + Send + Sync>
    } else {
        error!("Missing agent_model_api or agent_model_name in configuration");
        return;
    };

    // Initialize function tools
    let tools: Vec<Arc<dyn llm::function_tools::FunctionTool>> = vec![
        Arc::new(MathTool::new()),
        Arc::new(NaturalLanguageReplyTool::new(agent_llm.clone())),
        Arc::new(CodeWriterTool::new(agent_llm.clone())),
        // Note: ChatHistoryTool will be initialized later by BotAdapter with MessageStore
    ];

    info!("Initialized {} function tools", tools.len());

    // Create BrainAgent with LLM and tools
    let brain_agent = BrainAgent::new(agent_llm, tools, "傲娇".into());

    // Convert BrainAgent to AgentBox
    let agent_box: Option<Box<dyn bot_adapter::adapter::BrainAgentTrait>> = 
        Some(Box::new(brain_agent) as Box<dyn bot_adapter::adapter::BrainAgentTrait>);

    // Create and start the bot adapter
    let adapter_config = BotAdapterConfig::new(
        config.bot_server_url,
        config.bot_server_token,
        args.qq_id,
    )
    .with_redis_url(redis_url)
    .with_database_url(database_url)
    .with_redis_reconnect(
        config.redis_reconnect_max_attempts,
        config.redis_reconnect_interval_secs,
    )
    .with_mysql_reconnect(
        config.mysql_reconnect_max_attempts,
        config.mysql_reconnect_interval_secs,
    )
    .with_brain_agent(agent_box);

    let adapter = BotAdapter::new(adapter_config).await;
    let adapter = adapter.into_shared();
    info!("Bot adapter initialized, connecting to server...");
    if let Err(e) = BotAdapter::start(adapter).await {
        error!("Bot adapter error: {}", e);
    }
}
