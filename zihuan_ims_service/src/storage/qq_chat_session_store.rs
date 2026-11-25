use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::graph::{SessionClaim, SessionStateRef, SESSION_CLAIM_CONTEXT};
use zihuan_core::ims_bot_adapter::message_helpers::OutboundMessagePersistence;
use zihuan_core::runtime::block_async;

const LOG_PREFIX: &str = "[QqChatAgentService]";

/// Last-resort watchdog for leaked claims: a panicking or hung turn that never
/// released its session would otherwise keep every later message from this sender
/// stuck in the steer queue forever. The `catch_unwind` guard around the turn already
/// releases on panic; this TTL covers anything that slips past it. Must stay well
/// above the worst legitimate turn duration (several chained slow model calls).
const STALE_SESSION_CLAIM_TTL: Duration = Duration::from_secs(30 * 60);

/// A claimed-but-silent session is force-released after this long, so a hung turn cannot lock its
/// sender out until the next stale-claim steal. Kept well above the worst legitimate turn (a
/// chained sub-agent research task) and below [`STALE_SESSION_CLAIM_TTL`].
const CLAIM_WATCHDOG_AFTER: Duration = Duration::from_secs(30 * 60);

/// How often the watchdog looks for expired claims.
const WATCHDOG_POLL: Duration = Duration::from_secs(15);

/// Claims under watchdog supervision, keyed by `(deadline, claim token, session node)` so distinct
/// claims never collide. Expired entries are always removed, so the map stays bounded.
static WATCHED_CLAIMS: Mutex<BTreeMap<(Instant, u64, String), (Arc<SessionStateRef>, String)>> =
    Mutex::new(BTreeMap::new());

static WATCHDOG_STARTED: Once = Once::new();

pub(crate) fn try_claim_session(
    session: &Arc<SessionStateRef>,
    sender_id: &str,
) -> (bool, Option<u64>) {
    let (state, claimed) =
        block_async(session.try_claim_with_ttl(sender_id, None, Some(STALE_SESSION_CLAIM_TTL)));

    if claimed {
        let claim_token = state.claim_token();
        if let (Ok(ctx), Some(token)) = (SESSION_CLAIM_CONTEXT.try_with(Arc::clone), claim_token) {
            ctx.register_claim(SessionClaim {
                session_ref: session.clone(),
                sender_id: sender_id.to_string(),
                claim_token: token,
            });
        }
        if let Some(token) = claim_token {
            watch_claim(session, sender_id, token);
        }
        (true, claim_token)
    } else {
        (false, None)
    }
}

pub(crate) fn release_session(
    session: &Arc<SessionStateRef>,
    sender_id: &str,
    claim_token: Option<u64>,
) {
    if let Ok(ctx) = SESSION_CLAIM_CONTEXT.try_with(Arc::clone) {
        ctx.unregister_claim(&session.node_id, sender_id);
    }
    let released = block_async(session.release(sender_id, claim_token));
    info!("{LOG_PREFIX} Released session for {sender_id}: released={released}");
}

/// Supervise one claim: if it is still held by the same token after [`CLAIM_WATCHDOG_AFTER`], it
/// is force-released so the sender can be answered again.
fn watch_claim(session: &Arc<SessionStateRef>, sender_id: &str, claim_token: u64) {
    start_watchdog();
    let deadline = Instant::now() + CLAIM_WATCHDOG_AFTER;
    let key = (deadline, claim_token, session.node_id.clone());
    if let Ok(mut claims) = WATCHED_CLAIMS.lock() {
        claims.insert(key, (session.clone(), sender_id.to_string()));
    }
}

fn start_watchdog() {
    WATCHDOG_STARTED.call_once(|| {
        std::thread::Builder::new()
            .name("qq-session-watchdog".into())
            .spawn(|| loop {
                std::thread::sleep(WATCHDOG_POLL);
                release_expired_claims();
            })
            .expect("spawn session claim watchdog");
    });
}

fn release_expired_claims() {
    let expired: Vec<((Instant, u64, String), (Arc<SessionStateRef>, String))> = {
        let Ok(mut claims) = WATCHED_CLAIMS.lock() else {
            return;
        };
        let now = Instant::now();
        let due: Vec<_> = claims.keys().filter(|key| key.0 <= now).cloned().collect();
        due.into_iter()
            .filter_map(|key| claims.remove(&key).map(|value| (key, value)))
            .collect()
    };

    for ((_, claim_token, node), (session, sender_id)) in expired {
        // Token-guarded: a turn that finished normally or a newer claim on the same sender makes
        // this a no-op.
        if block_async(session.release(&sender_id, Some(claim_token))) {
            warn!(
                "{LOG_PREFIX} session claim watchdog: force-released {sender_id} on {node} after \
                 {:?} without a finishing turn",
                CLAIM_WATCHDOG_AFTER
            );
        } else {
            debug!("{LOG_PREFIX} session claim watchdog: {sender_id} claim {claim_token} is gone");
        }
    }
}

pub(crate) fn build_outbound_persistence(
    rdb_pool: Option<&RelationalDbConnection>,
    group_name: Option<&str>,
    sender_name: &str,
) -> OutboundMessagePersistence {
    OutboundMessagePersistence {
        rdb_pool: rdb_pool.cloned(),
        redis_ref: None,
        group_name: group_name.map(ToOwned::to_owned),
        sender_name: Some(sender_name.to_string()),
    }
}
