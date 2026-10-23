use std::sync::Arc;

use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};
use uuid::Uuid;

use crate::storage::role_avatar_store::{RoleAvatarData, RoleAvatarStore};
use crate::storage::{
    build_relational_db_connection_for_connection, load_connections, ConnectionConfig,
    ConnectionKind,
};

pub struct RdbRoleAvatarStore {
    connection: RelationalDbConnection,
}

impl RdbRoleAvatarStore {
    pub fn new(connection: RelationalDbConnection) -> Self {
        Self { connection }
    }

    pub async fn from_connection_id(
        connection_id: &str,
        connections: &[ConnectionConfig],
    ) -> Result<Self> {
        let connection =
            build_relational_db_connection_for_connection(connection_id, connections).await?;
        Ok(Self::new(connection))
    }

    pub async fn from_first_available_connection() -> Result<Self> {
        let connections = load_connections()?;
        let connection = connections
            .iter()
            .find(|connection| {
                matches!(connection.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_))
            })
            .ok_or_else(|| {
                Error::ValidationError(
                    "no database connection available for avatar storage".to_string(),
                )
            })?;
        Self::from_connection_id(&connection.id, &connections).await
    }
}

#[async_trait::async_trait]
impl RoleAvatarStore for RdbRoleAvatarStore {
    async fn save_avatar(
        &self,
        role_id: &str,
        file_name: Option<&str>,
        mime_type: &str,
        image_data: Vec<u8>,
    ) -> Result<String> {
        let existing_avatar_id = if role_id.is_empty() {
            None
        } else {
            self.get_avatar_by_role(role_id).await?.map(|avatar| avatar.id)
        };
        let avatar_id = existing_avatar_id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let file_name = file_name.map(str::to_string);

        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get mysql pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query(
                    "INSERT INTO role_avatar (id, role_id, file_name, mime_type, image_data, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, NOW(), NOW())
                     ON DUPLICATE KEY UPDATE
                     role_id = VALUES(role_id),
                     file_name = VALUES(file_name),
                     mime_type = VALUES(mime_type),
                     image_data = VALUES(image_data),
                     updated_at = NOW()",
                )
                .bind(&avatar_id)
                .bind(role_id)
                .bind(&file_name)
                .bind(mime_type)
                .bind(image_data)
                .execute(pool)
                .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get sqlite pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query(
                    "INSERT INTO role_avatar (id, role_id, file_name, mime_type, image_data, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, datetime('now'), datetime('now'))
                     ON CONFLICT(id) DO UPDATE SET
                     role_id = excluded.role_id,
                     file_name = excluded.file_name,
                     mime_type = excluded.mime_type,
                     image_data = excluded.image_data,
                     updated_at = datetime('now')",
                )
                .bind(&avatar_id)
                .bind(role_id)
                .bind(&file_name)
                .bind(mime_type)
                .bind(image_data)
                .execute(pool)
                .await?;
            }
        }

        Ok(avatar_id)
    }

    async fn get_avatar(&self, avatar_id: &str) -> Result<Option<RoleAvatarData>> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get mysql pool for avatar storage".to_string(),
                    )
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, role_id, file_name, mime_type, image_data FROM role_avatar WHERE id = ? LIMIT 1",
                )
                .bind(avatar_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get sqlite pool for avatar storage".to_string(),
                    )
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, role_id, file_name, mime_type, image_data FROM role_avatar WHERE id = ? LIMIT 1",
                )
                .bind(avatar_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
        }
    }

    async fn get_avatar_by_role(&self, role_id: &str) -> Result<Option<RoleAvatarData>> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get mysql pool for avatar storage".to_string(),
                    )
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, role_id, file_name, mime_type, image_data FROM role_avatar WHERE role_id = ? LIMIT 1",
                )
                .bind(role_id)
                .fetch_optional(pool)
                .await?;
                Ok(row.map(map_avatar_row))
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get sqlite pool for avatar storage".to_string(),
                    )
                })?;
                let row = sqlx::query_as::<_, (String, String, Option<String>, String, Vec<u8>)>(
                    "SELECT id, role_id, file_name, mime_type, image_data FROM role_avatar WHERE role_id = ? LIMIT 1",
                )
                .bind(role_id)
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
                    Error::ValidationError(
                        "failed to get mysql pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query("DELETE FROM role_avatar WHERE id = ?")
                    .bind(avatar_id)
                    .execute(pool)
                    .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get sqlite pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query("DELETE FROM role_avatar WHERE id = ?")
                    .bind(avatar_id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }

    async fn delete_avatar_by_role(&self, role_id: &str) -> Result<()> {
        match &self.connection {
            RelationalDbConnection::MySql(mysql) => {
                let pool = mysql.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get mysql pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query("DELETE FROM role_avatar WHERE role_id = ?")
                    .bind(role_id)
                    .execute(pool)
                    .await?;
            }
            RelationalDbConnection::Sqlite(sqlite) => {
                let pool = sqlite.pool.as_ref().ok_or_else(|| {
                    Error::ValidationError(
                        "failed to get sqlite pool for avatar storage".to_string(),
                    )
                })?;
                sqlx::query("DELETE FROM role_avatar WHERE role_id = ?")
                    .bind(role_id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }
}

pub async fn first_available_role_avatar_store() -> Result<Arc<dyn RoleAvatarStore>> {
    let store = RdbRoleAvatarStore::from_first_available_connection().await?;
    Ok(Arc::new(store))
}

fn map_avatar_row(row: (String, String, Option<String>, String, Vec<u8>)) -> RoleAvatarData {
    let (id, role_id, file_name, mime_type, image_data) = row;
    RoleAvatarData {
        id,
        role_id,
        file_name,
        mime_type,
        image_data,
    }
}
