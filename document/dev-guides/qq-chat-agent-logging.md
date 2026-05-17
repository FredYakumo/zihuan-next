# QQ Chat Agent Logging

This document explains the current task-scoped logging design for the live QQ chat agent.

Primary implementation files:

- `zihuan_service/src/agent/qq_chat_agent_logging.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`
- `zihuan_service/src/agent/classify_intent.rs`
- `zihuan_agent/src/brain.rs`

## Goals

The logging design exists to solve two problems at once:

- keep task logs readable for operators and developers
- preserve enough stage-level timing to debug slow or incorrect replies

The agent no longer dumps broad raw debug state by default. Instead, it emits a compact sequence of high-signal events and then a fixed timeline summary at task end.

## Design Overview

`qq_chat_agent_logging.rs` is the single owner of the QQ chat agent task trace format.

It provides two pieces:

- `QqChatTaskTrace`: an in-memory per-task trace aggregator
- `QqChatBrainObserver`: a `BrainObserver` adapter that converts Brain loop events into task-trace events

`qq_chat_agent_core.rs` remains the business orchestrator. It should only:

- create the trace at task start
- mark major business stages
- pass the observer into `Brain`
- call `finish_with_summary()` before leaving the task

This separation is intentional. Business code decides **when** a stage happens; the logging module decides **how** that stage is rendered.

## Logged Stages

The current trace captures these stage families:

- inbound message receipt
- task creation
- intent classification
- embedding participation inside intent classification
- final LLM request payload
- Brain tool request turns
- tool call start and finish
- final LLM result
- parsed assistant result
- outbound QQ message batch send
- task completion summary

Each key event uses a short readable log line with:

- stage title
- elapsed time in milliseconds
- compact payload preview

The final summary block uses absolute timestamps plus stage durations.

## Intent Classification Integration

`classify_intent.rs` exposes `classify_intent_with_trace(...)`.

That trace object reports:

- final category
- classification path: `local_guard`, `similarity_guard`, or `llm`
- whether embedding was used
- whether the intent LLM was used
- embedding duration when applicable
- total classification duration
- raw label text when classification reached the LLM

The QQ chat task trace stores this result and uses it to render both:

- the live “intent classified” log line
- the final timeline section

## Brain Integration

`zihuan_agent::brain::Brain` now supports an optional observer.

The observer receives:

- assistant tool-request turns
- tool start
- tool finish
- final assistant response / stop reason

The QQ chat agent does not depend on Brain text logs to reconstruct timing anymore. Instead, it uses `QqChatBrainObserver` so tool timing and final-result timing are recorded in one place.

Other Brain callers are unaffected because the observer is optional.

## Output Policy

The QQ chat task log should keep only high-signal content:

- user message
- intent result
- LLM conversation payload
- model output
- tool arguments
- tool results
- final outbound QQ message list
- final task summary

Avoid reintroducing top-level logs for:

- duplicate raw vs expanded message dumps
- large history payload snapshots
- large system prompt banner dumps
- repeated token-unavailable messages at multiple points
- duplicate tool call logs in both tool implementations and the task trace

If a new log line does not help answer “what did the agent receive, decide, call, return, and send?”, it usually does not belong in the main task trace.

## How To Extend

When adding a new observable stage:

1. Add the timing/state field to `QqChatTaskTraceInner`.
2. Add one public trace method on `QqChatTaskTrace`.
3. Call that method from `qq_chat_agent_core.rs` at the business boundary.
4. If the stage must appear in the final fixed summary, update `finish_with_summary()`.

Prefer extending the trace module instead of writing ad-hoc `info!` lines inside the business flow.

## Boundaries

This module is not a general metrics system.

It does not:

- persist extra structured task metadata outside existing logs
- change task JSONL schema
- expose reusable tracing primitives for every agent type

It is intentionally scoped to the QQ live reply path. If another service agent later needs the same pattern, copy the design, then extract only the genuinely shared parts.
