# QQMessage

This document explains the project's `QQMessage` model in detail, including its data structures, serde compatibility rules, runtime hydration behavior, and how it is consumed by the QQ agent and node system.

Code entry point:

- `zihuan_core/src/ims_bot_adapter/models/message.rs`

Related runtime paths:

- `ims_bot_adapter/src/adapter.rs`
- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

## Role In The System

`QQMessage` corresponds to `crate::ims_bot_adapter::models::message::Message` in Rust code. It represents a single QQ message segment, not a full message event.

The current implementation supports these segment types:

- `text`
- `at`
- `reply`
- `image`
- `forward`

A complete incoming message is usually represented as `Vec<QQMessage>`, for example:

```json
[
  { "type": "text", "data": { "text": "hello" } },
  { "type": "at", "data": { "qq": "123456" } },
  { "type": "reply", "data": { "id": "987654321" } }
]
```

In the node system, this type maps to:

- `DataType::QQMessage`
- `DataValue::QQMessage(QQMessage)`
- `Vec<QQMessage>`

## Rust Structure

The core enum is defined as:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    #[serde(rename = "at")]
    At(AtTargetMessage),
    #[serde(rename = "reply", alias = "replay")]
    Reply(ReplyMessage),
    #[serde(rename = "image")]
    Image(ImageMessage),
    #[serde(rename = "forward")]
    Forward(ForwardMessage),
}
```

This means the JSON payload uses an outer `type` field to select the variant, while variant-specific fields live under `data`.

In docs and node type names, this enum is usually called `QQMessage` to make its platform meaning explicit. When reading the codebase, keep in mind that:

- `QQMessage` in docs
- `QQMessage` in node type names
- `Message` in Rust implementation

all refer to the same conceptual type.

## Segment Types

### `text`

```rust
pub struct PlainTextMessage {
    pub text: String,
}
```

Purpose:

- Represents plain text content
- `Display` outputs the raw `text`
- `get_type()` returns `"text"`

Example:

```json
{ "type": "text", "data": { "text": "hello" } }
```

### `at`

```rust
pub struct AtTargetMessage {
    #[serde(rename = "qq", alias = "target")]
    #[serde(default, deserialize_with = "deserialize_option_string_from_string_or_number")]
    pub target: Option<String>,
}
```

Purpose:

- Represents an @-mention target
- Accepts either `target` or `qq` in input JSON
- Accepts either string or number input
- Normalizes everything to `Option<String>` internally

Compatibility behavior:

- `null` or missing field -> `None`
- `"123"` -> `Some("123".to_string())`
- `123` -> `Some("123".to_string())`

Display behavior:

- `Display` outputs `@{target}`
- If the target is missing, it outputs `@null`

Example:

```json
{ "type": "at", "data": { "qq": 123456 } }
```

### `reply`

```rust
pub struct ReplyMessage {
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub id: i64,
    #[serde(skip)]
    pub message_source: Option<Vec<Message>>,
}
```

Purpose:

- Represents a reply/reference to another message
- `id` accepts string or number input
- `message_source` is runtime-only hydrated context and is excluded from serde
- when present, `message_source` contains the full referenced message segment list

Compatibility behavior:

- `"123"` and `123` both deserialize into `i64`
- non-numeric strings fail deserialization

Display behavior:

- `Display` intentionally stays as `[Reply of message ID 123]`
- detailed referenced content is rendered by higher-level helpers, not by `Display`

Example:

```json
{ "type": "reply", "data": { "id": "987654321" } }
```

### `image`

```rust
pub struct ImageMessage {
    pub media: PersistedMedia,
}
```

Purpose:

- Represents an image segment
- points at a normalized persisted media object instead of loose locator fields

Runtime behavior:

- image segments are recursively normalized into `PersistedMedia`
- this normalization applies both to top-level messages and to images inside forwarded messages
- multimodal builders prefer `PersistedMedia.rustfs_path` and `PersistedMedia.mime_type`

For the full multimedia model, see `persisted-media.md`.

### `forward`

```rust
pub struct ForwardMessage {
    pub id: Option<String>,
    pub content: Vec<ForwardNodeMessage>,
}

pub struct ForwardNodeMessage {
    pub user_id: Option<String>,
    pub nickname: Option<String>,
    pub id: Option<String>,
    pub content: Vec<Message>,
}
```

Purpose:

- Represents a merged-forward / combined-forward message
- incoming events may contain only `id`
- detailed node content is hydrated later through NapCat `get_forward_msg`

Runtime behavior:

- forward hydration happens recursively in `ims_bot_adapter/src/adapter.rs`
- if a `ForwardMessage` has only `id`, the adapter calls `get_forward_msg(message_id)`
- the returned payload is parsed leniently:
  - node wrappers may be either `{ "data": ... }` or direct objects
  - node content may be under `content` or `message`
  - content may be:
    - a `Vec<Message>`-shaped array
    - a single message object
    - a CQ-code string such as text mixed with `[CQ:image,...]` and `[CQ:forward,...]`
- nested forwards are supported recursively

Display behavior:

- with only `id`: `[Forward of message ID ...]`
- with hydrated `content`: `[Forward with N node(s)]`
- readable expansion is handled by recursive rendering helpers, not by `Display`

## Serde Compatibility Strategy

QQ platform payloads and adapter inputs often vary between string values, numeric values, alias field names, and value-level shape differences. This file intentionally uses a "wide input, narrow internal representation" strategy.

### `deserialize_i64_from_string_or_number`

Used by `ReplyMessage.id`.

Accepts:

- JSON number
- JSON string

Rejects:

- object
- array
- bool
- non-numeric string

The goal is to guarantee that the internal representation is always `i64`.

### `deserialize_option_string_from_string_or_number`

Used by `AtTargetMessage.target` and `ForwardMessage.id`.

Accepts:

- `null`
- number
- string

The goal is to guarantee an internal `Option<String>`, which makes bot-id comparison, deduplication, and display logic simpler.

### Value-level forward parsing

`ims_bot_adapter/src/adapter.rs` contains additional non-derive parsing for forward payloads:

- CQ-code strings are parsed into `Vec<Message>`
- `[CQ:forward,id=...]` becomes `Message::Forward { id, content: [] }`
- nested CQ forwards are later hydrated recursively

This is intentionally more tolerant than plain `serde_json::from_value::<Vec<Message>>()`.

## `Display` Semantics

`Message` and all of its variants implement `Display`. This is not only for debugging or presentation. Some higher-level logic still uses these string forms directly.

Current output rules:

- `text` -> raw text
- `at` -> `@target`
- `reply` -> `[Reply of message ID ...]`
- `image` -> `[Image ...]`
- `forward` -> `[Forward of message ID ...]` or `[Forward with N node(s)]`

If you change `Display`, you are changing runtime behavior, not just UI wording. It directly affects:

- fallback text rendering in logs
- code paths still using `to_string()`
- debug output

However, current higher-level user-facing rendering no longer depends only on `Display`. For reply and forward messages, readable text is now built through recursive helpers such as `render_messages_readable()`.

## `MessageProp` Aggregation

`MessageProp` is not the raw message structure. It is a derived structure built from `Vec<QQMessage>` for intent detection and LLM consumption:

```rust
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
}
```

Field meanings:

- `content`: a human-readable string built through recursive rendering, including hydrated forward contents
- `ref_content`: contextual text extracted from `reply.message_source`, also rendered recursively
- `is_at_me`: whether the message mentions the bot itself
- `at_target_list`: deduplicated mention targets in first-seen order

### Construction Rules

`MessageProp::from_messages(messages, bot_id)` works roughly as follows:

1. Render the full message list via `render_messages_readable(messages)`
2. Collect all `AtTargetMessage.target` values
3. For each `reply`, if `message_source` exists, render it recursively into `ref_content`
4. If `bot_id` is provided, check whether it appears in `at_target_list`

### Example

Input:

```rust
vec![
    Message::PlainText(PlainTextMessage { text: "Hello".into() }),
    Message::At(AtTargetMessage { target: Some("42".into()) }),
]
```

Output is roughly:

```text
content = Some("Hello @42")
ref_content = None
is_at_me = true   // when bot_id == "42"
at_target_list = ["42"]
```

This structure affects reply decisions, context assembly, and what gets persisted into MySQL `content`.

## Hydration And Recursive Expansion

The runtime now has three distinct stages for reply / forward messages:

1. adapter hydration
2. readable rendering
3. QQ-agent inference expansion

### Adapter hydration

In `ims_bot_adapter/src/adapter.rs`:

- `reply` segments try to restore `message_source` from runtime cache or MySQL via `restore_message_snapshot()`
- `forward` segments hydrate `content` from NapCat `get_forward_msg`
- nested forwards are hydrated recursively
- images inside hydrated forwards are also recursively cached into object storage

This means the in-memory `MessageEvent` seen by downstream code may be much richer than the raw platform event.

### Readable rendering

In `zihuan_core/src/ims_bot_adapter/models/message.rs`, `render_messages_readable()` provides a recursive textual view:

- plain text and mentions render directly
- replies stay compact in-place but contribute full referenced text to `ref_content`
- forwards render as a block with sender-prefixed node content

This readable rendering is the basis for:

- `MessageProp.content`
- `MessageProp.ref_content`
- MySQL `content` persistence for new messages

### QQ-agent inference expansion

`zihuan_service/src/agent/qq_chat_agent_core.rs` adds a further agent-only step:

- `expand_event_for_inference()` clones the incoming `MessageEvent`
- `expand_messages_for_inference()` recursively replaces reply/forward shells with explicit text boundaries and inner content
- the expanded event is used for:
  - `build_user_message()`
  - `extract_user_message_text()`
  - the fixed `message_event` input passed into editable QQ-agent tool subgraphs

This is important because the agent should reason over the fully expanded referenced and forwarded content, while storage and raw logs may still preserve a more platform-shaped message list.

## Multimodal Extraction

Both of these paths now understand hydrated forwards:

- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

In both files:

- `Message::Forward` is rendered as an explicit `[转发内容]` block
- node content is traversed recursively
- images inside forward nodes can become multimodal `ContentPart`s

If the selected LLM reports `supports_multimodal_input = false`, the agent still cannot visually inspect image content. In that case it will receive expanded text and image placeholders, but not true image parts.

## Nested Forward Support

Nested forward messages are supported end-to-end in the runtime message handling path:

- CQ-code parsing can produce inner `Message::Forward` segments
- adapter hydration recursively expands nested forwards
- image caching recurses into nested forward nodes
- agent inference expansion recurses into nested forwards before calling the brain/LLM
- MySQL `MessageEvent` persistence stores the fully hydrated tree in `raw_message_json` when that persistence path is explicitly used

The practical limitation is not recursion depth in the current implementation, but whether upstream NapCat payloads remain parseable by the adapter's lenient value-level parser.

## Requirements When Extending `QQMessage`

If you add a new QQ message segment type, do not only extend the enum. At minimum, also check these places:

1. `zihuan_core/src/ims_bot_adapter/models/message.rs`
2. `fmt::Display for Message`
3. `MessageBase::get_type()`
4. `MessageProp::from_messages()`
5. adapter hydration if the new segment can appear inside forwards
6. multimodal builders in `ims_bot_adapter` and `qq_chat_agent`
7. persistence and restore behavior if the segment needs lossless reply reconstruction
8. every node that declares `DataType::QQMessage` or `Vec(QQMessage)`

Recommended constraints:

- keep serde input compatibility lenient for message types that still support compatibility; image segments are now intentionally strict
- do not let one unsupported field make the entire message unusable
- define `Display` semantics explicitly
- decide whether the new segment should affect `content`, `ref_content`, mention extraction, multimodal extraction, or persistence

## Relationship To The Node System

When docs mention `QQMessage`, that is usually the node-layer type name, not necessarily the exact Rust type name.

Examples:

- the `string_to_plain_text` node outputs a single `QQMessage`
- the `qq_message_list_data` node outputs `Vec<QQMessage>`
- some nodes forward `MessageEvent.message_list` to downstream nodes

In practice:

- the bot adapter layer converts platform input into `Vec<Message>`
- the node layer exposes the same data as `QQMessage` or `Vec<QQMessage>`

This is the same data represented with two naming layers.

## Current Limitations

At the current state of the code, `QQMessage` covers:

- plain text
- @ mentions
- reply references
- images
- merged forwards

It does not yet cover other common QQ segment types such as:

- emoji
- files
- voice

If upstream input starts including those types, you should first define:

- the JSON structure
- what text representation the LLM should see
- whether the node UI needs editing support
- whether runtime logic needs extra resource download or storage behavior

## Related Files

- `zihuan_core/src/ims_bot_adapter/models/message.rs`
- `zihuan_core/src/ims_bot_adapter/models/event_model.rs`
- `ims_bot_adapter/src/adapter.rs`
- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`
- `zihuan_graph_engine/src/message_restore.rs`

## Programmatic Sending Helpers

When code outside the node-graph needs to send QQ messages, use the helpers in `ims_bot_adapter/src/message_helpers.rs` rather than duplicating WebSocket action calls.

```rust
use ims_bot_adapter::message_helpers::{
    get_bot_id,
    send_friend_text,
    send_group_text,
    send_friend_batches,
    send_group_batches,
    send_group_progress_notification,
    send_friend_progress_notification,
};
```

| Function | Purpose |
|---|---|
| `get_bot_id(adapter)` | Returns the bot's own QQ ID as a `String` |
| `send_friend_text(adapter, target_id, text)` | Sends a plain-text message to a friend |
| `send_group_text(adapter, target_id, text)` | Sends a plain-text message to a group |
| `send_friend_batches(adapter, target_id, batches)` | Sends `Vec<Vec<Message>>` batches to a friend |
| `send_group_batches(adapter, target_id, batches)` | Sends `Vec<Vec<Message>>` batches to a group |
| `send_group_progress_notification(adapter, group_id, mention_id, content)` | Sends `@mention + text` to a group |
| `send_friend_progress_notification(adapter, target_id, content)` | Sends a plain progress message to a friend |

All functions handle Tokio context automatically via `block_in_place` and are safe to call from synchronous node code running on a Tokio worker thread.
