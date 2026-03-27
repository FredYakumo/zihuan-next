# QQMessage

This document explains the project's `QQMessage` model in detail, including its data structures, serde compatibility rules, and how it is consumed by the runtime and node system.

Code entry point:

- `src/bot_adapter/models/message.rs`

## Role In The System

`QQMessage` corresponds to `crate::bot_adapter::models::message::Message` in Rust code. It represents a single QQ message segment, not a full message event.

The current implementation supports only three segment types:

- `text`
- `at`
- `reply`

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
    #[serde(alias = "qq")]
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
    pub message_source: Option<Box<Message>>,
}
```

Purpose:

- Represents a reply/reference to another message
- `id` accepts string or number input
- `message_source` is runtime-only context and is excluded from serde

Compatibility behavior:

- `"123"` and `123` both deserialize into `i64`
- Non-numeric strings fail deserialization

Display behavior:

- With only `id`: `[Reply of message ID 123]`
- With `message_source`: `[Reply of message ID 123: xxx]`

Example:

```json
{ "type": "reply", "data": { "id": "987654321" } }
```

## Serde Compatibility Strategy

QQ platform payloads and adapter inputs often vary between string values, numeric values, and alias field names. This file intentionally uses a "wide input, narrow internal representation" strategy.

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

### `deserialize_option_i64_from_string_or_number`

This helper exists in the file, but is not currently used by any active field.

It is suitable for fields that are:

- optional
- possibly provided as string or number by upstream input
- meant to become `Option<i64>` internally

### `deserialize_option_string_from_string_or_number`

Used by `AtTargetMessage.target`.

Accepts:

- `null`
- number
- string

The goal is to guarantee an internal `Option<String>`, which makes later bot-id comparison, deduplication, and display serialization simpler.

## `Display` Semantics

`Message` and all of its variants implement `Display`. This is not only for debugging or presentation. Higher-level logic uses these string forms directly when building LLM-readable text.

Current output rules:

- `text` -> raw text
- `at` -> `@target`
- `reply` -> `[Reply of message ID ...]`

If you change `Display`, you are changing runtime behavior, not just UI wording. It directly affects:

- `MessageProp::from_messages()` output in `content`
- the text sent to the LLM
- logs and debug output

## `MessageProp` Aggregation

`MessageProp` is not the raw message structure. It is a derived structure built from `Vec<QQMessage>` for intent detection and LLM consumption:

```rust
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>
}
```

Field meanings:

- `content`: a human-readable string built by joining each segment's `Display` output
- `ref_content`: contextual text extracted from `reply.message_source`
- `is_at_me`: whether the message mentions the bot itself
- `at_target_list`: deduplicated mention targets in first-seen order

### Construction Rules

`MessageProp::from_messages(messages, bot_id)` works roughly as follows:

1. Iterate over `Vec<QQMessage>`
2. Call `to_string()` on each segment and join them with a single space into `content`
3. Collect all `AtTargetMessage.target` values
4. For `reply`, if `message_source` exists, collect its `Display` output and join them with newlines into `ref_content`
5. If `bot_id` is provided, check whether it appears in `at_target_list` and compute `is_at_me`

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

This structure affects reply decisions, context assembly, and whether referenced content should be forwarded into the LLM context.

## Requirements When Extending `QQMessage`

If you add a new QQ message segment type, do not only extend the enum. At minimum, also check these places:

1. `src/bot_adapter/models/message.rs`
2. `fmt::Display for Message`
3. `MessageBase::get_type()`
4. `MessageProp::from_messages()`
5. every node that declares `DataType::QQMessage` or `Vec(QQMessage)`
6. JSON docs and examples

Recommended constraints:

- Keep serde input compatibility lenient
- Do not let one unsupported field make the entire message unusable
- Define the `Display` text semantics explicitly
- Decide whether the new segment should affect `content`, `ref_content`, mention extraction, or trigger logic

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

At the current state of the code, `QQMessage` only covers:

- plain text
- @ mentions
- reply references

It does not yet cover other common QQ segment types such as:

- images
- emoji
- files
- voice
- forward / merged-forward messages

If upstream input starts including those types, you should first define:

- the JSON structure
- what text representation the LLM should see
- whether the node UI needs editing support
- whether runtime logic needs extra resource download or storage behavior

## Related Files

- `src/bot_adapter/models/message.rs`
- `src/bot_adapter/models/event_model.rs`
- `src/node/data_type.rs`
- `src/node/data_value.rs`
- `src/node/util/qq_message_list_data.rs`
- `src/node/util/string_to_plain_text.rs`
