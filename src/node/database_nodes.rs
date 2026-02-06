use crate::error::Result;
use crate::node::data_value::{RedisConfig, MySqlConfig};
use crate::node::{DataType, DataValue, Node, Port};
use crate::config::pct_encode;
use std::collections::HashMap;
use std::sync::Arc;

/// Redis configuration node - builds Redis connection config from input ports
pub struct RedisNode {
    id: String,
    name: String,
}

impl RedisNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for RedisNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Redis连接配置 - 构建Redis连接URL并输出引用")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("redis_host", DataType::String)
                .with_description("Redis主机地址"),
            Port::new("redis_port", DataType::Integer)
                .with_description("Redis端口号"),
            Port::new("redis_db", DataType::Integer)
                .with_description("Redis数据库编号 (默认: 0)")
                .optional(),
            Port::new("redis_password", DataType::String)
                .with_description("Redis密码")
                .optional(),
            Port::new("reconnect_max_attempts", DataType::Integer)
                .with_description("最大重连次数 (默认: 3)")
                .optional(),
            Port::new("reconnect_interval_secs", DataType::Integer)
                .with_description("重连间隔秒数 (默认: 60)")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("redis_ref", DataType::RedisRef)
                .with_description("Redis连接配置引用"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Extract required parameters
        let host = inputs.get("redis_host").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("redis_host is required".to_string()))?;
        
        let port = inputs.get("redis_port").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u16),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("redis_port is required".to_string()))?;
        
        let db = inputs.get("redis_db").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u8),
            _ => None,
        }).unwrap_or(0);
        
        let password = inputs.get("redis_password").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        });

        // Build URL from components
        let url = if let Some(pw) = password {
            if !pw.is_empty() {
                let enc = pct_encode(&pw);
                Some(format!("redis://:{}@{}:{}/{}", enc, host, port, db))
            } else {
                Some(format!("redis://{}:{}/{}", host, port, db))
            }
        } else {
            Some(format!("redis://{}:{}/{}", host, port, db))
        };

        let max_attempts = inputs.get("reconnect_max_attempts").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u32),
            _ => None,
        });
        let interval_secs = inputs.get("reconnect_interval_secs").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u64),
            _ => None,
        });

        let config = RedisConfig {
            url,
            reconnect_max_attempts: max_attempts,
            reconnect_interval_secs: interval_secs,
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "redis_ref".to_string(),
            DataValue::RedisRef(Arc::new(config)),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// MySQL configuration node - builds MySQL connection config from input ports
pub struct MySqlNode {
    id: String,
    name: String,
}

impl MySqlNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MySqlNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("MySQL连接配置 - 构建MySQL连接URL并输出引用")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("mysql_host", DataType::String)
                .with_description("MySQL主机地址"),
            Port::new("mysql_port", DataType::Integer)
                .with_description("MySQL端口号"),
            Port::new("mysql_user", DataType::String)
                .with_description("MySQL用户名"),
            Port::new("mysql_password", DataType::String)
                .with_description("MySQL密码"),
            Port::new("mysql_database", DataType::String)
                .with_description("MySQL数据库名"),
            Port::new("reconnect_max_attempts", DataType::Integer)
                .with_description("最大重连次数 (默认: 3)")
                .optional(),
            Port::new("reconnect_interval_secs", DataType::Integer)
                .with_description("重连间隔秒数 (默认: 60)")
                .optional(),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("mysql_ref", DataType::MySqlRef)
                .with_description("MySQL连接配置引用"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        // Extract required parameters
        let host = inputs.get("mysql_host").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_host is required".to_string()))?;
        
        let port = inputs.get("mysql_port").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u16),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_port is required".to_string()))?;
        
        let user = inputs.get("mysql_user").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_user is required".to_string()))?;
        
        let password = inputs.get("mysql_password").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_password is required".to_string()))?;
        
        let database = inputs.get("mysql_database").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        }).ok_or_else(|| crate::error::Error::InvalidNodeInput("mysql_database is required".to_string()))?;

        // Build URL from components
        let url = if !password.is_empty() {
            let enc = pct_encode(&password);
            Some(format!("mysql://{}:{}@{}:{}/{}", user, enc, host, port, database))
        } else {
            Some(format!("mysql://{}@{}:{}/{}", user, host, port, database))
        };

        let max_attempts = inputs.get("reconnect_max_attempts").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u32),
            _ => None,
        });
        let interval_secs = inputs.get("reconnect_interval_secs").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u64),
            _ => None,
        });

        let config = MySqlConfig {
            url,
            reconnect_max_attempts: max_attempts,
            reconnect_interval_secs: interval_secs,
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "mysql_ref".to_string(),
            DataValue::MySqlRef(Arc::new(config)),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
