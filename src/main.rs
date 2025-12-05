mod bot_adapter;

use std::fs;
use serde::Deserialize;
use log::{info, error};
use log_util::log_util::LogUtil;
use lazy_static::lazy_static;

use bot_adapter::adapter::BotAdapter;

lazy_static! {
    static ref BASE_LOG: LogUtil = LogUtil::new_with_path("zihuan_next_aibot", "logs");
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(rename = "BOT_SERVER_URL")]
    bot_server_url: Option<String>,
    #[serde(rename = "BOT_SERVER_TOKEN")]
    bot_server_token: Option<String>,
}

fn load_config() -> Config {
    // Try to load from config.yaml
    match fs::read_to_string("config.yaml") {
        Ok(content) => {
            match serde_yaml::from_str(&content) {
                Ok(config) => {
                    info!("Loaded configuration from config.yaml");
                    config
                }
                Err(e) => {
                    error!("Failed to parse config.yaml: {}", e);
                    Config {
                        bot_server_url: None,
                        bot_server_token: None,
                    }
                }
            }
        }
        Err(e) => {
            info!("Could not read config.yaml ({}), using environment variables", e);
            Config {
                bot_server_url: None,
                bot_server_token: None,
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging using LogUtil
    LogUtil::init_with_logger(&BASE_LOG).expect("Failed to initialize logger");

    info!("zihuan_next_aibot-800b starting...");

    // Load configuration from config.yaml, fallback to environment variables
    let config = load_config();
    
    let bot_server_url = config.bot_server_url
        .or_else(|| std::env::var("BOT_SERVER_URL").ok())
        .unwrap_or_else(|| "ws://localhost:3001".to_string());
    
    let bot_server_token = config.bot_server_token
        .or_else(|| std::env::var("BOT_SERVER_TOKEN").ok())
        .unwrap_or_default();

    // Create and start the bot adapter
    let adapter = BotAdapter::new(bot_server_url, bot_server_token);
    
    info!("Bot adapter initialized, connecting to server...");
    
    if let Err(e) = adapter.start().await {
        error!("Bot adapter error: {}", e);
    }
}
