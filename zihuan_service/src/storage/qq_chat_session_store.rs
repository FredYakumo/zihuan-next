use std::sync::Arc;

use log::info;

use ims_bot_adapter::message_helpers::OutboundMessagePersistence;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::data_value::{SessionClaim, SessionStateRef, SESSION_CLAIM_CONTEXT};

const LOG_PREFIX: &str = "[QqChatAgent]";

pub(crate) fn try_claim_session(
    session: &Arc<SessionStateRef>,
    sender_id: &str,
) -> (bool, Option<u64>) {
    let (state, claimed) = block_async(session.try_claim(sender_id, None));

    if claimed {
        let claim_token = state.claim_token();
        if let (Ok(ctx), Some(token)) = (SESSION_CLAIM_CONTEXT.try_with(Arc::clone), claim_token) {
            ctx.register_claim(SessionClaim {
                session_ref: session.clone(),
                sender_id: sender_id.to_string(),
                claim_token: token,
            });
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

pub(crate) fn build_outbound_persistence(
    mysql_ref: Option<&Arc<MySqlConfig>>,
    group_name: Option<&str>,
    sender_name: &str,
) -> OutboundMessagePersistence {
    OutboundMessagePersistence {
        mysql_ref: mysql_ref.cloned(),
        redis_ref: None,
        group_name: group_name.map(ToOwned::to_owned),
        sender_name: Some(sender_name.to_string()),
    }
}
