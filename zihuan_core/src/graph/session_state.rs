use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::Mutex as TokioMutex;

tokio::task_local! {
    pub static SESSION_CLAIM_CONTEXT: Arc<SessionClaimContext>;
}

#[derive(Debug, Clone)]
pub struct SessionStateEntry {
    pub in_session: bool,
    pub state_json: Value,
    pub(crate) claim_token: Option<u64>,
    /// When the claim was taken; backs the staleness watchdog in `try_claim_with_ttl`.
    pub(crate) claimed_at: Option<std::time::Instant>,
}

impl SessionStateEntry {
    pub fn claim_token(&self) -> Option<u64> {
        self.claim_token
    }
}

#[derive(Clone)]
pub struct SessionStateRef {
    pub node_id: String,
    pub entries: Arc<TokioMutex<HashMap<String, SessionStateEntry>>>,
    pub next_claim_token: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct SessionClaim {
    pub session_ref: Arc<SessionStateRef>,
    pub sender_id: String,
    pub claim_token: u64,
}

#[derive(Debug, Default)]
pub struct SessionClaimContext {
    claims: Mutex<HashMap<(String, String), SessionClaim>>,
}

impl SessionClaimContext {
    pub fn register_claim(&self, claim: SessionClaim) {
        self.claims
            .lock()
            .unwrap()
            .insert((claim.session_ref.node_id.clone(), claim.sender_id.clone()), claim);
    }

    pub fn unregister_claim(&self, session_node_id: &str, sender_id: &str) -> Option<SessionClaim> {
        self.claims
            .lock()
            .unwrap()
            .remove(&(session_node_id.to_string(), sender_id.to_string()))
    }

    pub fn claim_token_for(&self, session_node_id: &str, sender_id: &str) -> Option<u64> {
        self.claims
            .lock()
            .unwrap()
            .get(&(session_node_id.to_string(), sender_id.to_string()))
            .map(|claim| claim.claim_token)
    }

    pub fn drain_claims(&self) -> Vec<SessionClaim> {
        self.claims.lock().unwrap().drain().map(|(_, claim)| claim).collect()
    }
}

impl SessionStateRef {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            entries: Arc::new(TokioMutex::new(HashMap::new())),
            next_claim_token: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get_state(&self, sender_id: &str) -> SessionStateEntry {
        self.entries.lock().await.get(sender_id).cloned().unwrap_or(SessionStateEntry {
            in_session: false,
            state_json: Value::Null,
            claim_token: None,
            claimed_at: None,
        })
    }

    pub async fn clear_state(&self, sender_id: &str) -> bool {
        self.entries.lock().await.remove(sender_id).is_some()
    }

    pub async fn try_claim(
        &self,
        sender_id: &str,
        state_json: Option<Value>,
    ) -> (SessionStateEntry, bool) {
        self.try_claim_with_ttl(sender_id, state_json, None).await
    }

    /// Claim with a staleness watchdog. When `stale_ttl` is set and the existing claim
    /// is older than the TTL, the claim is treated as leaked (its owning task panicked
    /// or hung without releasing) and is stolen. `release` is token-guarded, so a late
    /// release from the stolen owner cannot undo the new claim.
    pub async fn try_claim_with_ttl(
        &self,
        sender_id: &str,
        state_json: Option<Value>,
        stale_ttl: Option<std::time::Duration>,
    ) -> (SessionStateEntry, bool) {
        let mut entries = self.entries.lock().await;
        if let Some(existing) = entries.get(sender_id) {
            if existing.in_session {
                let expired = match (existing.claimed_at, stale_ttl) {
                    (Some(claimed_at), Some(ttl)) => claimed_at.elapsed() > ttl,
                    _ => false,
                };
                if !expired {
                    return (existing.clone(), false);
                }
                log::warn!(
                    "Session claim for {sender_id} on {} held past {:?}; treating it as leaked and stealing it",
                    self.node_id, stale_ttl
                );
            }
        }

        let claim_token = self.next_claim_token.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let entry = SessionStateEntry {
            in_session: true,
            state_json: state_json.unwrap_or(Value::Null),
            claim_token: Some(claim_token),
            claimed_at: Some(std::time::Instant::now()),
        };
        entries.insert(sender_id.to_string(), entry.clone());
        (entry, true)
    }

    pub async fn release(&self, sender_id: &str, claim_token: Option<u64>) -> bool {
        let mut entries = self.entries.lock().await;
        let Some(existing) = entries.get(sender_id) else {
            return false;
        };

        if let Some(expected_token) = claim_token {
            if existing.claim_token != Some(expected_token) {
                return false;
            }
        }

        entries.insert(
            sender_id.to_string(),
            SessionStateEntry {
                in_session: false,
                state_json: Value::Null,
                claim_token: None,
                claimed_at: None,
            },
        );
        true
    }
}

impl fmt::Debug for SessionStateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionStateRef")
            .field("node_id", &self.node_id)
            .field("entries", &"<TokioMutex<HashMap<...>>>")
            .finish()
    }
}
