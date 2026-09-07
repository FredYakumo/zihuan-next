use tokio::sync::mpsc::UnboundedSender;

use crate::model_inference::llm::StreamToken;

/// Transport out boundary of a RoleService turn (documents/transport.md).
///
/// Procedures emit streamed inference tokens and structured turn events through this sink
/// without knowing the concrete transport (SSE for the Workspace dashboard, message sending
/// for QQ). The transport adapter owns the receiving side and forwards to the external channel.
pub trait TransportSink: Send + Sync {
    /// Fresh sender for streamed inference tokens; the brain procedure pumps deltas through it.
    fn token_sender(&self) -> UnboundedSender<StreamToken>;

    /// Structured turn event (tool calls, context compaction, ...).
    fn send_event(&self, event: serde_json::Value);
}
