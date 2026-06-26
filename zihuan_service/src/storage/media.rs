use std::collections::HashMap;

use ims_bot_adapter::models::message::{Message, PersistedMedia};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::message_restore::query_media_by_id;

/// Resolves media references for all messages in the given batches.
///
/// Iterates over every batch and every message within each batch, replacing
/// placeholder `Image` markers with the corresponding persisted media from
/// `available_media` or from persistent storage. Returns an error if any
/// media_id cannot be resolved.
pub(crate) fn resolve_media_references(
    batches: &mut [Vec<Message>],
    available_media: &HashMap<String, PersistedMedia>,
    rdb_pool: Option<&RelationalDbConnection>,
) -> Result<()> {
    for batch in batches {
        for message in batch {
            resolve_message_media_reference(message, available_media, rdb_pool)?;
        }
    }
    Ok(())
}

/// Resolves a single message's media reference, replacing placeholder `Image`
/// markers with actual persisted media data.
///
/// Resolution order:
/// 1. If the image already has a rustfs_path or original_source, it is left as-is.
/// 2. Otherwise, looks up the `media_id` in the provided `available_media` map.
/// 3. Falls back to restoring the media from persistent storage via `query_media_by_id`.
///
/// Recursively resolves media references inside `Forward` message nodes.
/// Non-image messages are ignored.
fn resolve_message_media_reference(
    message: &mut Message,
    available_media: &HashMap<String, PersistedMedia>,
    rdb_pool: Option<&RelationalDbConnection>,
) -> Result<()> {
    match message {
        Message::Image(image) => {
            if image.rustfs_path().is_some() || image.original_source().is_some() {
                return Ok(());
            }

            let media_id = image.media.media_id.trim();
            if media_id.is_empty() {
                return Err(Error::ValidationError("outbound image marker is missing media_id".to_string()));
            }

            if let Some(media) = available_media.get(media_id) {
                image.media = media.clone();
                return Ok(());
            }

            if let Some(media) = query_media_by_id(media_id, rdb_pool)? {
                image.media = media;
                return Ok(());
            }

            Err(Error::ValidationError(format!(
                "failed to resolve outbound image media_id '{}'",
                media_id
            )))
        }
        Message::Forward(forward) => {
            for node in &mut forward.content {
                for nested in &mut node.content {
                    resolve_message_media_reference(nested, available_media, rdb_pool)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
