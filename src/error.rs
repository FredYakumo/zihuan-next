use std::io;
use std::num::ParseFloatError;
use redis::RedisError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("{0}")]
    StringError(String),
    
    #[error("{0}")]
    StaticStrError(&'static str),
    
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
    
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("HTTP header error: {0}")]
    HttpHeader(#[from] http::Error),
    
    #[error("Serde JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Serde YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    
    #[error("Parse float error: {0}")]
    ParseFloat(#[from] ParseFloatError),
    
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Helper to convert from String to Error
impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::StringError(s)
    }
}

// Helper to convert from &str to Error
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::StringError(s.to_string())
    }
}

#[macro_export]
macro_rules! string_error {
    ($($arg:tt)*) => {
        $crate::error::Error::StringError(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => {
        return Err($crate::string_error!($($arg)*))
    };
}
