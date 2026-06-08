use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use crate::ims_bot_adapter::models::MessageEvent;
use crate::llm::{LLMMessage, MessagePart};

pub const STEER_PREFIX: &str = "[User-STEER Message] User steer a new message:";

/// Processing instruction appended to user messages, telling the model how to
/// respond to the current turn.
pub const PROCESSING_INSTRUCTION: &str = "[Processing Instructions]\n\
     - This is the user's message the assistant needs to process.\n\
     - Ensure your reply addresses the user's request directly and naturally.";

/// Apply the steer prefix to an already-built `LLMMessage`.
///
/// Prepends `STEER_PREFIX` to the message parts.
pub fn apply_steer_prefix(mut message: LLMMessage, _api_style: Option<&str>) -> LLMMessage {
    if message.parts.is_empty() {
        message.parts.push(MessagePart::text(STEER_PREFIX));
        return message;
    }

    match message.parts.first_mut() {
        Some(MessagePart::Text { text }) => {
            *text = format!("{STEER_PREFIX}\n\n{text}");
        }
        _ => {
            message
                .parts
                .insert(0, MessagePart::text(format!("{STEER_PREFIX}\n\n")));
        }
    }

    message
}

/// A pending steer/interrupt event waiting to be injected into the agent's
/// conversation.
#[derive(Debug, Clone)]
pub struct PendingSteerEvent {
    pub event: MessageEvent,
    pub time: String,
}

#[derive(Debug, Default)]
struct PendingSteerSession {
    queue: VecDeque<PendingSteerEvent>,
    accepted_steer_count: usize,
}

/// Thread-safe store of pending steer events, keyed by sender ID.
///
/// Each sender has at most `max_steer_count` accepted events; once that limit
/// is reached, further events are rejected. Events are drained (all at once or
/// oldest-first) before the next agent inference turn.
#[derive(Default)]
pub struct PendingSteerStore {
    by_sender: Mutex<HashMap<String, PendingSteerSession>>,
}

impl PendingSteerStore {
    /// Enqueue a steer event for the given sender, subject to the per-sender
    /// `max_steer_count` limit. Returns `(accepted, queue_len, accepted_count)`.
    pub fn enqueue_with_limit(
        &self,
        sender_id: &str,
        pending: PendingSteerEvent,
        max_steer_count: usize,
    ) -> (bool, usize, usize) {
        let mut guard = self.by_sender.lock().unwrap();
        let session = guard.entry(sender_id.to_string()).or_default();
        if session.accepted_steer_count >= max_steer_count {
            return (false, session.queue.len(), session.accepted_steer_count);
        }
        session.accepted_steer_count += 1;
        session.queue.push_back(pending);
        (true, session.queue.len(), session.accepted_steer_count)
    }

    /// Drain all pending events for the given sender.
    /// Returns `(drained_events, remaining_queue_len, accepted_steer_count)`.
    pub fn drain_all(&self, sender_id: &str) -> (Vec<PendingSteerEvent>, usize, usize) {
        let mut guard = self.by_sender.lock().unwrap();
        let Some(session) = guard.get_mut(sender_id) else {
            return (Vec::new(), 0, 0);
        };
        let drained: Vec<PendingSteerEvent> = session.queue.drain(..).collect();
        let remaining_queue_len = session.queue.len();
        let accepted_steer_count = session.accepted_steer_count;
        if session.queue.is_empty() && session.accepted_steer_count == 0 {
            guard.remove(sender_id);
        }
        (drained, remaining_queue_len, accepted_steer_count)
    }

    /// Pop the oldest pending event for the given sender.
    pub fn pop_oldest(&self, sender_id: &str) -> Option<(PendingSteerEvent, usize, usize)> {
        let mut guard = self.by_sender.lock().unwrap();
        let session = guard.get_mut(sender_id)?;
        let popped = session.queue.pop_front()?;
        let remaining_queue_len = session.queue.len();
        let accepted_steer_count = session.accepted_steer_count;
        if session.queue.is_empty() && session.accepted_steer_count == 0 {
            guard.remove(sender_id);
        }
        Some((popped, remaining_queue_len, accepted_steer_count))
    }

    /// Mark the sender's steering session as finished (reset accepted count).
    /// Cleans up the entry if the queue is already empty.
    pub fn finish_session(&self, sender_id: &str) {
        let mut guard = self.by_sender.lock().unwrap();
        if let Some(session) = guard.get_mut(sender_id) {
            session.accepted_steer_count = 0;
            if session.queue.is_empty() {
                guard.remove(sender_id);
            }
        }
    }

    /// Ensure a session entry exists for the given sender (created on demand).
    pub fn ensure_session_entry(&self, sender_id: &str) {
        let mut guard = self.by_sender.lock().unwrap();
        guard.entry(sender_id.to_string()).or_default();
    }
}

/// Merge multiple pending steer events into a single `MessageEvent` by
/// concatenating their message lists (preserving order).
pub fn build_merged_follow_up_event(pending_events: &[PendingSteerEvent]) -> MessageEvent {
    let first_event = pending_events
        .first()
        .expect("merged follow-up requires at least one pending steer event");
    let mut merged_event = first_event.event.clone();
    merged_event.message_list = pending_events
        .iter()
        .flat_map(|pending| pending.event.message_list.clone())
        .collect();
    merged_event
}

/// Compatibility shim retained so call sites can migrate without carrying
/// provider-specific state inside `LLMMessage`.
pub fn message_with_api_style(message: LLMMessage, _api_style: Option<&str>) -> LLMMessage {
    message
}
