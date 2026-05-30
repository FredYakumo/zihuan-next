# QQ Chat Agent Reply And Send Flow

This document describes how `qq_chat_agent` turns the Brain's final output into QQ messages and routes everything through the unified sender in `zihuan_service/src/agent/qq_chat_agent_msg_send.rs`.

Main implementation files:

- `zihuan_service/src/agent/qq_chat_agent_msg_send.rs`
- `zihuan_service/src/agent/qq_chat_agent_claimed.rs`
- `zihuan_service/src/agent/tools/reply_message.rs`

## Overview

The unified pipeline has three layers:

1. After the Brain loop ends, extract the last sendable Assistant text from `brain_output`.
2. Collect image `media_id`s returned by tools and read the reply directive written by the `reply_message` tool.
3. Let `qq_chat_agent_msg_send.rs` handle all QQ-specific post-processing:
   - text normalization
   - `@sender` / `@QQ` parsing
   - `[Image media_id=...]` parsing
   - `[no reply]` suppression
   - text splitting
   - single-image / multi-image / mixed text+image rules
   - forward planning
   - reply attachment
   - final send + persistence

Upper layers no longer assemble QQ batches directly.

## Model Output Contract

The model may still output these special markers directly:

- `@sender`
- `@123456`
- `[Image media_id=media-xxxx]`
- `[no reply]`

The model must no longer output:

- `[Reply his_message]`
- `[Reply message_id=...]`

If it needs to quote a message, it must call the built-in `reply_message` tool.

## `reply_message` Tool

`reply_message` is a QQ-chat-only default built-in tool and can be toggled in the frontend.

Parameters:

- `message_id?: integer`

Behavior:

- With `message_id`: reply to that QQ message.
- Without `message_id`: reply to the message that triggered the current agent turn.
- The tool does not send anything immediately. It only writes a turn-scoped reply directive.
- The actual reply segment is attached later by `qq_chat_agent_msg_send.rs`.

## Unified Planning Rules

### Stage 1: Normalize Assistant Text

- In group chats, replace `@sender` with `@<sender_id>`.
- Detect `[no reply]`; if present, return `suppress_send=true`.
- Parse `[Image media_id=...]`.
- Parse inline `@QQ` mentions.

### Stage 2: Split Text

Text still uses the existing segmentation + length-limit logic, including code fence and quote repair.

Rules:

- If the text becomes 1 or 2 chunks, plain messages are allowed.
- If the text becomes 3 or more chunks, the body must be sent as a forward message.

### Stage 3: Image Rules

- One image: send directly.
- More than one image: must use forward.

### Stage 4: Mixed Text / Image / Text

The unified planner preserves original order.

If either condition is true:

- total text chunks >= 3
- image count > 1

then the remaining body is converted into one `ForwardMessage`.

### Stage 5: Lift `@` And Reply Outside Forward

In forward mode:

- `@` messages are not placed inside the forward payload.
- the reply segment produced by `reply_message` is not placed inside the forward payload either.

The planner first finds a carrier batch for reply / mentions:

1. Prefer the first independently sendable text chunk extracted from the forward body.
2. If there is no text, use the first image.
3. If there is neither text nor image, send a reply-only carrier.

Typical outcomes:

- `Reply + @ + text`
- `Reply + image`
- then send the remaining forward body

## Send Entrypoints

These scenarios now all route through `qq_chat_agent_msg_send.rs`:

- final Brain reply
- command echo text
- `/task` detail side effect
- long-task start notification
- long-task completion notification
- built-in tool progress notifications
- editable tool / `tool_subgraph` progress notifications

Low-level packet sending still reuses `ims_bot_adapter::message_helpers`, but QQ-chat-specific splitting, forward, reply, and mention policy lives only in the agent layer.

## Persistence

- Successful sends still go through the existing outbound persistence path.
- Conversation history no longer stores the old `[Reply ...]` text protocol.
- The visible assistant text stored in history is based on the unified send planner, so old Reply markers are not preserved.

## Key Types

### `QqReplyDirective`

- `Explicit { message_id }`
- `TriggerMessage`

Represents which message the final reply should quote.

### `QqSendContext`

Unified send context with:

- adapter
- target_id
- is_group
- group_name
- bot_id / bot_name
- mention_target_id
- persistence
- max_text_chars

### `QqOutboundPlan`

Unified planning result with:

- `batches: Vec<Vec<Message>>`
- `suppress_send: bool`
- `visible_text: Option<String>`

## Maintenance Rule

- New QQ-chat outbound behavior should be implemented in `qq_chat_agent_msg_send.rs`.
- Do not maintain separate split / forward / reply rule sets in `qq_chat_agent_claimed.rs`, `tools/common.rs`, or `tool_subgraph.rs`.
- If a future feature needs new QQ send semantics, extend the unified plan and planner instead of adding another side path.
