# QQ Chat Agent Steer

This document is the canonical technical reference for the QQ chat agent steer mechanism.

It describes how same-sender overlapping messages are handled while one QQ chat reply flow is still active.

## Purpose

The steer mechanism exists so the live QQ chat agent can accept mid-reply user corrections without opening a second concurrent reply flow for the same sender.

Instead of modeling overlap as “agent busy”, the runtime models it as “the user is steering an unfinished reply”.

## Scope

This document covers:

- sender-overlap detection during an active QQ chat reply flow
- pending-steer queue behavior
- injection into an unfinished Brain run
- automatic follow-up continuation when injection is missed
- history persistence rules
- configured steer limits
- logging and observability boundaries

It does not redefine:

- group-chat gating rules
- initial QQ message ingestion or inbox architecture
- general Brain observer design

Those topics remain in their dedicated documents.

## Runtime Model

For one sender, the QQ chat agent still allows only one active reply flow at a time.

When another message from that same sender arrives before the current flow finishes:

1. The new message is recognized as a steer candidate.
2. The message is queued in the sender-local pending-steer buffer owned by the QQ chat agent runtime.
3. The current reply flow decides later whether that queued steer can still enter the unfinished Brain conversation, or whether it should become the next follow-up turn.

This preserves single-sender serialization while still allowing in-flight user correction.

## Steer Lifecycle

### 1. Acceptance

A same-sender overlapping message is accepted as steer only while a reply flow is already active for that sender.

The accepted steer does not open a second task immediately. It is attached to the current active flow first.

### 2. Queueing

Accepted steer messages enter a pending-steer queue associated with the sender session.

The queue is ordered by arrival time.

### 3. Injection Window

If the current Brain run finishes tool calls and is about to enter another inference round, the runtime drains queued steer messages before that next inference.

When several steer messages are waiting in the same injection window, they are merged in arrival order into one explicit `user` message.

The model therefore sees a normal user interruption, not a hidden control directive.

### 4. Missed Injection

If a steer message arrives after the current reply flow has already passed its last usable inference boundary, that steer is not discarded just because it missed the current round.

Instead, once the current reply flow completes, the queued steer becomes the primary input of the next automatic follow-up turn.

### 5. Consumption And Persistence

When steer is actually consumed, the consumed input is appended to saved conversation history.

If multiple queued steer messages were merged into one injected interruption, the saved history records that merged explicit user message.

## Merging Semantics

Steer merging is intentionally narrow:

- merging only happens inside a live reply flow
- merging only happens when another Brain inference round is about to begin
- merging preserves message arrival order
- merging produces one explicit injected `user` message

The runtime does not inject each queued steer message separately when they land in the same injection window.

## Important Boundary

The steer mechanism is not a debounce window before the first reply.

The first incoming user message still starts the active reply flow immediately.

The runtime does not wait to see whether the user will send more text before starting the first inference.

Because of that, multi-message merging happens only after the reply flow is already in progress and only if another inference round is still ahead.

## Limits And Configuration

One active reply flow accepts only a limited number of steer messages.

That limit is configured by `QqChatAgentConfig.max_steer_count`.

Current default:

- `max_steer_count = 4`

When the active flow has already accepted the configured maximum number of steer messages, additional same-sender overlapping messages are dropped.

## Task And Follow-Up Behavior

The steer mechanism changes how overlap is interpreted, but it does not change the task model:

- one active reply flow still corresponds to one concrete handled reply task
- steer first belongs to that active reply flow
- only when the current flow finishes and a queued steer starts continuation does it become the next automatic follow-up turn

This is why same-sender overlap is no longer treated as a “busy conflict” path.

## Logging And Observability

Steer has two main observability surfaces:

- task-trace events for steer receipt, steer injection, and follow-up continuation
- service-level queue-management logs for enqueue, drop, and dequeue decisions

Important logging fields include:

- `message_id`
- `steer_count`
- `injected_messages`
- configured `max_steer_count`
- remaining queue length when relevant
- truncated steer payload preview
- `merged=true/false` on injection events

When several steer messages collapse into one injected interruption, `steer_count` is greater than `injected_messages`.

Detailed trace-format guidance remains in [`qq-chat-agent-logging.md`](qq-chat-agent-logging.md).

## Related Documents

- [`qq-message.md`](qq-message.md) for QQ message expansion and user-visible behavior context
- [`qq-chat-agent-logging.md`](qq-chat-agent-logging.md) for task-trace event design
- [`../program-execute-flow.md`](../program-execute-flow.md) for overall runtime and service boundaries
