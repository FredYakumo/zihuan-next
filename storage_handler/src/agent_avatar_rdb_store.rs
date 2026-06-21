use std::sync::Arc;

use uuid::Uuid;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};

use crate::agent_avatar_store::{AgentAvatarData, AgentAvatarStore};
use crate::{build_relational_db_connection_for_connection, load_connections, ConnectionConfig, ConnectionKind};

pub struct RdbAgentAvatarStore {
    connection: RelationalDbConnection,
}

impl RdbAgentAvatarStore {
    pub fn new(connection: RelationalDbConnection) -> Self {
        Self { connection }
    }

    pub async fn from_connection_id(connection_id: &str, connections: &[ConnectionConfig]) -> Result<Self> {
        let connection = build_relational_db_connection_for_connection(connection_id, connections).await?;
        Ok(Self::new(connection))
    }

    pub async fn from_first_available_connection() -> Result<Self> {
        let connections = load_connections()?;
        let connection = connections
            .iter()
            .find(|connection| matches!(connection.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_)))
            .ok_or_else(|| Error::ValidationError("no database connection available for avatar storage".to_string()))?;
        Self::from_connection_id(&connection.id, &connections).await
    }
}

#[async_trait::async_trait]
impl AgentAvatarStore for RdbAgentAvatarStore {
    async fn save_avatar(
        &self,
        agent_id: &str,
        file_name: Option<&str>,
        mime_type: &str,
        image_data: Vec<u8>,
    ) -> Result<String> {
        let existing_avatar_id = self.get_avatar_by_agent(agent_id).await?.map(|avatar| avatar.id);
        let avatar_id = existing_avatar_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let file_name = file_name.map(str::to_string);

        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get mysql pool for avatar storage".to_string())
                })?;
                sqlx::query(
                    "INSERT INTO agent_avatar (id, agent_id, file_name, mime_type, image_data, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, NOW(), NOW())
                     ON DUPLICATE KEY UPDATE
                     agent_id = VALUES(agent_id),
                     file_name = VALUES(file_name),
                     mime_type = VALUES(mime_type),
                     image_data = VALUES(image_data),
                     updated_at = NOW()",
                )
                .bind(&avatar_id)
                .bind(agent_id)
                .bind(&file_name)
                .bind(mime_type)
                .bind(image_data)
                .execute(pool)
                .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get sqlite pool for avatar storage".to_string())
                })?;
                sqlx::query(
                    "INSERT INTO agent_avatar (id, agent_id, file_name, mime_type, image_data, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))
                     ON CONFLICT(id) DO UPDATE SET
                     agent_id = excluded.agent_id,
                     file_name = excluded.file_name,
                     mime_type = excluded.mime_type,
                     image_data = excluded.image_data,
                     updated_at = datetime('now')",
                )
                .bind(&avatar_id)
                .bind(agent_id)
                .bind(&file_name)
                .bind(mime_type)
                .bind(image_data)
                .execute(pool)
                .await?;
            }
        }

        Ok(avatar_id)
    }

    async fn get_avatar(&self, avatar_id: &str) -> Result<Option<AgentAvatarData>> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get mysql pool for avatar storage".to_string())
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, agent_id, file_name, mime_type, image_data FROM agent_avatar WHERE id = ? LIMIT 1",
                )
                .bind(avatar_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get sqlite pool for avatar storage".to_string())
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, agent_id, file_name, mime_type, image_data FROM agent_avatar WHERE id = ? LIMIT 1",
                )
                .bind(avatar_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
        }
    }

    async fn get_avatar_by_agent(&self, agent_id: &str) -> Result<Option<AgentAvatarData>> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get mysql pool for avatar storage".to_string())
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, agent_id, file_name, mime_type, image_data FROM agent_avatar WHERE agent_id = ? LIMIT 1",
                )
                .bind(agent_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get sqlite pool for avatar storage".to_string())
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, agent_id, file_name, mime_type, image_data FROM agent_avatar WHERE agent_id = ? LIMIT 1",
                )
                .bind(agent_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
        }
    }

    async fn delete_avatar(&self, avatar_id: &str) -> Result<()> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get mysql pool for avatar storage".to_string())
                })?;
                sqlx::query("DELETE FROM agent_avatar WHERE id = ?")
                    .bind(avatar_id)
                    .execute(pool)
                    .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get sqlite pool for avatar storage".to_string())
                })?;
                sqlx::query("DELETE FROM agent_avatar WHERE id = ?")
                    .bind(avatar_id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }

    async fn delete_avatar_by_agent(&self, agent_id: &str) -> Result<()> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get mysql pool for avatar storage".to_string())
                })?;
                sqlx::query("DELETE FROM agent_avatar WHERE agent_id = ?")
                    .bind(agent_id)
                    .execute(pool)
                    .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError("failed to get sqlite pool for avatar storage".to_string())
                })?;
                sqlx::query("DELETE FROM agent_avatar WHERE agent_id = ?")
                    .bind(agent_id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }
}

pub async fn first_available_agent_avatar_store() -> Result<Arc<dyn AgentAvatarStore>> {
    let store = RdbAgentAvatarStore::from_first_available_connection().await?;
    Ok(Arc::new(store))
}

fn map_avatar_row(row: (String, String, Option<String>, String, Vec<u8>)) -> AgentAvatarData {
    let (id, agent_id, file_name, mime_type, image_data) = row;
    AgentAvatarData {
        id,
        agent_id,
        file_name,
        mime_type,
        image_data,
    }
}
