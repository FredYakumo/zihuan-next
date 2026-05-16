# PersistedMedia

Describes the unified media handling model in `zihuan-next`. All image segments within QQ messages reference a normalized `PersistedMedia` object. This ensures that message persistence, multimodal inference, image sending, and image search consume a single, stable set of media facts.

## In This Document

- [Type Reference](#type-reference)
- [Design Rules](#design-rules)
- [Inbound Flow](#inbound-flow)
- [Multimodal Inference](#multimodal-inference)
- [Persistence Model](#persistence-model)
- [Outbound Image Sending](#outbound-image-sending)
- [Weaviate Image Collection](#weaviate-image-collection)
- [Error Behavior](#error-behavior)
- [Guidelines](#guidelines)
- [Related Documents](#related-documents)

## Type Reference

### PersistedMediaSource

Specifies the origin category of a media object.

```rust
pub enum PersistedMediaSource {
    Upload,
    QqChat,
    WebSearch,
}
```

| Variant | Description | Serde Value |
|---------|-------------|-------------|
| `Upload` | Media created or provided by the local system or manual upload paths | `"upload"` |
| `QqChat` | Media originating from QQ / NapCat message events | `"qq_chat"` |
| `WebSearch` | Media originating from Tavily or other web-search image flows | `"web_search"` |

Serde serialization uses `snake_case` naming.

### PersistedMedia

The primary media descriptor. All downstream image logic reads from this struct.

```rust
pub struct PersistedMedia {
    pub media_id: String,
    pub source: PersistedMediaSource,
    pub original_source: String,
    pub rustfs_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}
```

| Field | Type | Description |
|-------|------|-------------|
| `media_id` | `String` | Stable media identity. Used by runtime logic and Weaviate image storage. |
| `source` | `PersistedMediaSource` | Normalized source category. |
| `original_source` | `String` | The original upstream locator string from the source platform. |
| `rustfs_path` | `String` | RustFS object key. Primary file locator within the system. Empty string before persistence completes. |
| `name` | `Option<String>` | Human-readable media name. |
| `description` | `Option<String>` | Textual description of the image content. |
| `mime_type` | `Option<String>` | MIME type of the media. Used for multimodal construction and outbound behavior. Preferred over filename-based inference. |

### ImageMessage

Wraps a `PersistedMedia` for inclusion in the message tree.

```rust
pub struct ImageMessage {
    pub media: PersistedMedia,
}
```

**Remarks:**

- Message-level image semantics are expressed as `Message::Image(ImageMessage)`.
- Media-level identity and storage facts are expressed as `PersistedMedia`.
- The message layer and media layer are intentionally separate. A single `PersistedMedia` instance may be referenced by multiple messages or tool paths.

## Design Rules

### Single Source of Truth

After normalization, downstream code must not reconstruct image identity from the following legacy fields:

- `file`
- `path`
- `url`
- `object_key`
- `object_url`
- `local_path`

Downstream code must read media facts exclusively from `PersistedMedia`:

- `image.media.rustfs_path`
- `image.media.original_source`
- `image.media.name`
- `image.media.description`
- `image.media.mime_type`

### Strict Image JSON Format

The current image JSON format requires the `media` field. Legacy payloads that only contain fields such as `file`, `url`, `object_key`, or `object_url` are treated as incompatible.

```json
{
  "type": "image",
  "data": {
    "media": {
      "media_id": "...",
      "source": "qq_chat",
      "original_source": "...",
      "rustfs_path": "...",
      "name": "...",
      "description": "...",
      "mime_type": "image/jpeg"
    }
  }
}
```

## Inbound Flow

### 1. QQ Event Parsing

**Module:** `ims_bot_adapter/src/adapter.rs`

Parses incoming platform payloads into `Vec<Message>`. For image segments, creates an initial `PersistedMedia` with the following defaults:

| Field | Initial Value |
|-------|---------------|
| `source` | `QqChat` |
| `original_source` | Upstream QQ locator string |
| `rustfs_path` | `""` (empty) |
| `name` | If available from the event |
| `mime_type` | `None` |

At this stage, the media object exists but is not yet persisted.

### 2. RustFS Persistence and Media Normalization

**Module:** `storage_handler/src/object_storage/media_cache.rs`

This module is the normalization boundary for image persistence. It performs the following steps:

1. Resolve image bytes from the current image source.
2. Infer or preserve the content type.
3. Upload the bytes to RustFS.
4. Construct a normalized `PersistedMedia`.
5. Write the result back into `ImageMessage.media`.

After this step, the following invariants hold:

- `rustfs_path` is populated with a valid object key.
- `mime_type` is set when content type detection succeeds.
- `media_id` is a stable value.

### 3. Forward Message Recursion

Image normalization recurses into forwarded messages. Images nested inside hydrated forward nodes are converted to `PersistedMedia` objects before any downstream inference or storage processes them.

## Multimodal Inference

Two modules construct multimodal inputs from image messages:

- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

Both follow the same resolution priority:

| Priority | Source | Condition |
|----------|--------|-----------|
| 1 | `rustfs_path` | Default path. Read bytes from RustFS. |
| 2 | `original_source` | Fallback when RustFS is unavailable. |
| 3 | `media.mime_type` | Used when constructing `data:` URLs. |
| 4 | `media.name` | Secondary naming signal only. |

### MIME Type Handling

`PersistedMedia.mime_type` is the authoritative source for content type information. Filename-based MIME inference is unreliable and must not be used as a primary mechanism.

**Example of the failure mode in legacy code:**

- Actual bytes: JPEG
- `name`: `"download"` (no extension)
- Extension-based fallback: incorrectly produces `image/png`

The current model avoids this by storing `mime_type` on `PersistedMedia` and reusing it across all paths.

### Data URL Construction

When multimodal input is enabled, image content is converted to `ContentPart::ImageUrl` using one of the following formats:

- `data:<mime>;base64,...` — constructed from resolved bytes and `PersistedMedia.mime_type`
- Direct data URL — when the source already provides one

The MIME type in the data URL must be sourced from `PersistedMedia.mime_type` when available.

## Persistence Model

### Runtime Cache

**Module:** `zihuan_graph_engine/src/message_restore.rs`

Maintains an in-process cache keyed by `message_id`, with `Vec<Message>` as the value. Preserves full image messages including `PersistedMedia`.

### Redis Cache

Redis stores a structured `CachedMessageSnapshotPayload` rather than plain rendered text:

```rust
pub struct CachedMessageSnapshotPayload {
    pub message_id: String,
    pub content: String,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
}
```

| Field | Content |
|-------|---------|
| `media_json` | Direct media metadata, including `media_id` |
| `raw_message_json` | Full message tree, including `ImageMessage { media }` |

Messages restored from Redis can recover image segments with full `PersistedMedia` data.

### MySQL

**Module:** `zihuan_graph_engine/src/message_persistence.rs`

Persists the following columns:

| Column | Content |
|--------|---------|
| `content` | Rendered text |
| `media_json` | Serialized media records derived from `PersistedMedia` |
| `raw_message_json` | Full serialized `Vec<Message>` tree |

`collect_media_records(...)` extracts image records from `PersistedMedia`, including: `media_id`, `source`, `original_source`, `rustfs_path`, `name`, `description`, `mime_type`.

### Restore Order

`restore_message_snapshot(message_id)` resolves in the following priority:

| Priority | Source | Fidelity |
|----------|--------|----------|
| 1 | Runtime in-process cache | Full message tree |
| 2 | Redis `CachedMessageSnapshotPayload` | Full message tree |
| 3 | MySQL `raw_message_json` | Full message tree |
| 4 | MySQL `content` + `media_json` | Reconstructed; partial fidelity |

`media_id` survives at all tiers, including Redis cache hits.

## Outbound Image Sending

**Module:** `ims_bot_adapter/src/ws_action.rs`

Converts a normalized `ImageMessage` back to a QQ outbound API payload. Resolution order:

| Priority | Source | Condition |
|----------|--------|-----------|
| 1 | `original_source` | Contains base64 or data URL |
| 2 | `original_source` | Is a local file path |
| 3 | `rustfs_path` | RustFS object key |
| 4 | `original_source` | Remote URL |

Outbound behavior is driven by `PersistedMedia`, not by legacy image fields.

## Weaviate Image Collection

The image collection schema models persisted media records, not message-bound image rows.

### Properties

| Property | Type | Source |
|----------|------|--------|
| `media_id` | string | `PersistedMedia.media_id` |
| `original_source` | string | `PersistedMedia.original_source` |
| `rustfs_path` | string | `PersistedMedia.rustfs_path` |
| `name` | string | `PersistedMedia.name` |
| `description` | string | `PersistedMedia.description` |
| `mime_type` | string | `PersistedMedia.mime_type` |
| `source` | string | `PersistedMedia.source` |

### Named Vectors

The image collection uses Weaviate named vectors to maintain separate vector spaces for `name` and `description`:

| Vector Name | Source | Description |
|-------------|--------|-------------|
| `description_vector` | Text embedding of `PersistedMedia.description` | Primary query vector; used by default for semantic retrieval |
| `name_vector` | Text embedding of `PersistedMedia.name` | Used when performing semantic search by name |

### Upsert Behavior

`WeaviateRef::upsert_image_record(...)` accepts a `PersistedMedia`, a `description_vector`, and an optional `name_vector`. Weaviate object properties are derived directly from the `PersistedMedia` struct, and the named vectors are generated by the caller via an embedding model, keeping image search aligned with the same media facts used by QQ messages, multimodal builders, and outbound sending.

## Error Behavior

### Incompatible Image JSON

When the deserializer encounters legacy image JSON that does not contain the `media` field:

1. Logs via `error!` macro.
2. Returns a deserialization error.
3. No silent conversion is performed.

### Reconstruction Fallback

When `raw_message_json` is absent but `media_json` exists, the system reconstructs image messages from `MessageMediaRecord`. This fallback preserves `PersistedMedia` identity but with lower fidelity than a full `raw_message_json` restore.

## Guidelines

1. **Treat `PersistedMedia` as the single source of truth.** Do not derive image identity from legacy fields.
2. **Do not introduce independent locator fields into `ImageMessage`.** All file location and identity information must reside in `PersistedMedia`.
3. **Preserve `mime_type` when bytes are resolved.** Do not rely on filename-based content type inference.
4. **Prefer `raw_message_json` for lossless restore.** Use the full message tree when exact restoration is required.
5. **Update both inference paths simultaneously.** Any change to media behavior that affects multimodal input must be applied to both `extract_message_from_event.rs` and `qq_chat_agent_core.rs`.

## Related Documents

- [QQ Message](qq-message.md)
- [QQ Message Storage](qq_message_storage.md)
