//! Avatar storage trait and implementations for agent avatars.

use zihuan_core::error::Result;

/// Avatar data for retrieval
#[derive(Debug, Clone)]
pub struct AgentAvatarData {
    pub id: String,
    pub agent_id: String,
    pub file_name: Option<String>,
    pub mime_type: String,
    pub image_data: Vec<u8>,
}

/// Trait for agent avatar storage operations
#[async_trait::async_trait]
pub trait AgentAvatarStore: Send + Sync {
    /// Save or update avatar for an agent
    /// Returns the avatar ID
    async fn save_avatar(
        &self,
        agent_id: &str,
        file_name: Option<&str>,
        mime_type: &str,
        image_data: Vec<u8>,
    ) -> Result<String>;


    async fn get_avatar(&self, avatar_id: &str) -> Result<Option<AgentAvatarData>>;


    async fn get_avatar_by_agent(&self, agent_id: &str) -> Result<Option<AgentAvatarData>>;


    async fn delete_avatar(&self, avatar_id: &str) -> Result<()>;

    async fn delete_avatar_by_agent(&self, agent_id: &str) -> Result<()>;
}
