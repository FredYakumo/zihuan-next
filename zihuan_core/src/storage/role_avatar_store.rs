//! Avatar storage trait and implementations for role service avatars.

use crate::error::Result;

/// Avatar data for retrieval
#[derive(Debug, Clone)]
pub struct RoleAvatarData {
    pub id: String,
    pub role_id: String,
    pub file_name: Option<String>,
    pub mime_type: String,
    pub image_data: Vec<u8>,
}

/// Trait for role service avatar storage operations
#[async_trait::async_trait]
pub trait RoleAvatarStore: Send + Sync {
    /// Save or update avatar for a role service
    /// Returns the avatar ID
    async fn save_avatar(
        &self,
        role_id: &str,
        file_name: Option<&str>,
        mime_type: &str,
        image_data: Vec<u8>,
    ) -> Result<String>;

    async fn get_avatar(&self, avatar_id: &str) -> Result<Option<RoleAvatarData>>;

    async fn get_avatar_by_role(&self, role_id: &str) -> Result<Option<RoleAvatarData>>;

    async fn delete_avatar(&self, avatar_id: &str) -> Result<()>;

    async fn delete_avatar_by_role(&self, role_id: &str) -> Result<()>;
}
