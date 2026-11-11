//! Inbound HTTP webhooks for IM bridges that push events instead of exposing a socket.
//!
//! WeChatPadPro (personal WeChat, iPad protocol) has no persistent connection to the adapter:
//! it POSTs each message to a URL we hand it. The route below is that URL; it only forwards the
//! raw payload into the running backend instance, which owns all protocol translation.

use salvo::prelude::*;
use salvo::writing::Json;
use serde_json::{json, Value};

/// `POST /api/ims/wechat_pad/{token}` — deliver one bridge webhook payload to its adapter.
///
/// The token is the secret path segment configured as `webhook_token` on the IM connection; it
/// is also the registry key the backend registers itself under when it starts.
#[handler]
pub async fn wechat_pad_webhook(req: &mut Request, res: &mut Response) {
    let token = req.param::<String>("token").unwrap_or_default();
    let payload = match req.parse_json::<Value>().await {
        Ok(payload) => payload,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(json!({"ok": false, "error": format!("invalid json body: {error}")})));
            return;
        }
    };

    let delivered =
        zihuan_core::ims_bot_adapter::runtime::wechat_pad::dispatch_webhook(&token, payload);
    if delivered {
        res.render(Json(json!({"ok": true})));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(json!({
            "ok": false,
            "error": "no running wechat_pad adapter for this webhook token",
        })));
    }
}
