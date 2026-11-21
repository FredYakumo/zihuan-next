//! Explicit Planer input; the empty event below is only a legacy context carrier, never an
//! inbound message: it has no sender or message identity and is never persisted as received.
use super::model::QqChatAgentService;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter;
use zihuan_core::ims_bot_adapter::models::{
    event_model::{MessageType, Sender, SenderGender},
    message::{ImageMessage, PersistedMedia},
    sender_model::{FriendSender, GroupSender, Sender as ReplyTarget},
    MessageEvent,
};
use zihuan_core::ims_bot_adapter::multimodal_image_url::resolve_image_message_part;
use zihuan_core::message_part::MessagePart;
use zihuan_core::role::planer::{PlanerCall, PlanerContext};

pub(crate) struct ImsPlanerInput {
    pub source_planer_id: String,
    pub prompt: String,
    pub target_type: String,
    pub target_id: String,
    /// Conversation images the Planer explicitly forwarded with the call, in call order.
    pub media: Vec<PersistedMedia>,
    pub context: PlanerContext,
    pub delivery: Mutex<Value>,
    pub cancellation: Arc<dyn zihuan_core::agent::AgentCancellation>,
}
impl ImsPlanerInput {
    /// Media lookup for the outbound `[Image media_id=...]` placeholder resolution.
    pub(crate) fn media_map(&self) -> HashMap<String, PersistedMedia> {
        self.media.iter().map(|media| (media.media_id.clone(), media.clone())).collect()
    }
    /// A group destination/session key is not a person. Private recall keeps the
    /// recipient's conversation scope, without treating the task as user input.
    pub(crate) fn memory_scope_person_id(&self) -> Option<&str> {
        (self.target_type == "private").then_some(self.target_id.as_str())
    }
    /// The IMS destination this task delivers to, as the same sender union an ordinary turn
    /// uses. A Planer task has no inbound member, so the destination itself is the target and
    /// its member fields stay empty. `None` when the call names no target: the role then only
    /// hands its result back to the Planer instead of posting it.
    pub(crate) fn reply_target(&self) -> Option<ReplyTarget> {
        if self.target_id.trim().is_empty() {
            return None;
        }
        Some(if self.target_type == "group" {
            ReplyTarget::Group(GroupSender {
                user_id: String::new(),
                nickname: String::new(),
                card: String::new(),
                role: None,
                group_id: self.target_id.clone(),
                group_name: None,
                gender: SenderGender::Unknown,
            })
        } else {
            ReplyTarget::Friend(FriendSender {
                user_id: self.target_id.clone(),
                nickname: String::new(),
                card: String::new(),
                role: None,
                gender: SenderGender::Unknown,
            })
        })
    }
}
struct RunningIms {
    service: Weak<QqChatAgentService>,
    adapter: SharedBotAdapter,
}
fn registry() -> &'static Mutex<HashMap<String, RunningIms>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, RunningIms>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
pub(super) struct Registration {
    id: String,
    service: Weak<QqChatAgentService>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        let mut entries = registry().lock().unwrap();
        if entries.get(&self.id).is_some_and(|entry| entry.service.ptr_eq(&self.service)) {
            entries.remove(&self.id);
        }
    }
}
pub(super) fn register(
    id: &str,
    service: &Arc<QqChatAgentService>,
    adapter: SharedBotAdapter,
) -> Registration {
    registry().lock().unwrap().insert(
        id.into(),
        RunningIms {
            service: Arc::downgrade(service),
            adapter,
        },
    );
    Registration {
        id: id.into(),
        service: Arc::downgrade(service),
    }
}
/// Ordinary turns share read access; Planer messages reserve their own conversation.
pub(crate) fn conversation_gate(
    role_id: &str,
    target_type: &str,
    target_id: &str,
) -> Arc<zihuan_core::role::turn_queue::TurnQueue> {
    static GATES: OnceLock<Mutex<HashMap<String, Weak<zihuan_core::role::turn_queue::TurnQueue>>>> =
        OnceLock::new();
    let key = format!("{role_id}:{target_type}:{target_id}");
    let mut gates = GATES.get_or_init(Default::default).lock().unwrap();
    gates.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = gates.get(&key).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(zihuan_core::role::turn_queue::TurnQueue::default());
    gates.insert(key, Arc::downgrade(&gate));
    gate
}
fn is_running(role_id: &str, service: &Arc<QqChatAgentService>) -> bool {
    registry()
        .lock()
        .unwrap()
        .get(role_id)
        .is_some_and(|entry| entry.service.ptr_eq(&Arc::downgrade(service)))
}
struct PlanerCancellation {
    role_id: String,
    service: Weak<QqChatAgentService>,
    context: PlanerContext,
}
impl zihuan_core::agent::AgentCancellation for PlanerCancellation {
    fn is_cancelled(&self) -> bool {
        self.context.is_cancelled()
            || !registry()
                .lock()
                .unwrap()
                .get(&self.role_id)
                .is_some_and(|entry| entry.service.ptr_eq(&self.service))
    }
}
pub async fn execute(
    role_id: &str,
    call: PlanerCall,
    media: HashMap<String, PersistedMedia>,
    context: PlanerContext,
) -> Result<Value> {
    let (service, adapter) = {
        let registry = registry().lock().unwrap();
        let entry = registry
            .get(role_id)
            .ok_or_else(|| zihuan_core::string_error!("IMS Role 未运行"))?;
        (
            entry
                .service
                .upgrade()
                .ok_or_else(|| zihuan_core::string_error!("IMS Role 已停止"))?,
            entry.adapter.clone(),
        )
    };
    let source_planer_id = context.planer_id.clone();
    let media = call.media_ids.iter().filter_map(|id| media.get(id.trim()).cloned()).collect();
    let input = Arc::new(ImsPlanerInput {
        source_planer_id,
        prompt: call.prompt.clone(),
        target_type: call.target_type.clone().unwrap_or_default(),
        target_id: call.target_id.clone().unwrap_or_default(),
        media,
        cancellation: Arc::new(PlanerCancellation {
            role_id: context.role_id.clone(),
            service: Arc::downgrade(&service),
            context: context.clone(),
        }),
        context,
        delivery: Mutex::new(Value::Null),
    });
    let session_key =
        format!("planer:{}:{}:{}", input.source_planer_id, input.target_type, input.target_id);
    let gate = conversation_gate(role_id, "planer", &session_key);
    let pending_destination = gate.clone().reserve(false);
    tokio::pin!(pending_destination);
    let destination_guard = loop {
        if input.cancellation.is_cancelled() || !is_running(role_id, &service) {
            return Err(zihuan_core::string_error!("IMS Role 已停止"));
        }
        tokio::select! {
            guard = &mut pending_destination => break guard,
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }
    };
    // Use the normal FIFO session claim mechanism for this Planer conversation.
    let claim_token = loop {
        let session = service.config.session.clone();
        let key = session_key.clone();
        let (claimed, token) = tokio::task::spawn_blocking(move || {
            super::super::storage::qq_chat_session_store::try_claim_session(&session, &key)
        })
        .await
        .map_err(|e| zihuan_core::string_error!("IMS session claim failed: {e}"))?;
        if claimed {
            break token;
        }
        if input.cancellation.is_cancelled() || !is_running(role_id, &service) {
            return Err(zihuan_core::string_error!("IMS Role 已停止"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    struct ClaimGuard {
        service: Arc<QqChatAgentService>,
        key: String,
        token: Option<u64>,
    }
    impl Drop for ClaimGuard {
        fn drop(&mut self) {
            crate::storage::qq_chat_session_store::release_session(
                &self.service.config.session,
                &self.key,
                self.token,
            );
        }
    }
    let claim = ClaimGuard {
        service: service.clone(),
        key: session_key.clone(),
        token: claim_token,
    };
    tokio::task::spawn_blocking(move || {
        let _claim = claim;
        let _destination_guard = destination_guard;
        zihuan_core::agent::runtime_context::with_current_agent_runtime_context(
            zihuan_core::agent::runtime_context::AgentRuntimeContext::from_resources(
                super::resources::QqChatRoleServiceResources::new(
                    service.config.qq_chat_config.clone(),
                )
                .into_shared(),
            ),
            || process_message(&service, &adapter, &input, &session_key),
        )
    })
    .await
    .map_err(|e| zihuan_core::string_error!("IMS Planer 消息处理中断：{e}"))?
}
fn process_message(
    service: &QqChatAgentService,
    adapter: &SharedBotAdapter,
    input: &ImsPlanerInput,
    session_key: &str,
) -> Result<Value> {
    let operation = service.config.cache.operation_lock(session_key);
    let _operation = zihuan_core::runtime::block_async(operation.lock());
    let is_group = input.target_type == "group";
    let event = MessageEvent {
        message_id: String::new(),
        message_type: if is_group {
            MessageType::Group
        } else {
            MessageType::Private
        },
        sender: Sender {
            user_id: String::new(),
            nickname: "Planer".to_string(),
            card: String::new(),
            role: None,
            gender: SenderGender::Unknown,
        },
        message_list: Vec::new(),
        group_id: is_group.then(|| input.target_id.clone()),
        group_name: None,
        is_group_message: is_group,
    };
    let mut ctx = service.turn_context(adapter, &event);
    ctx.planer = Some(input);
    ctx.tool_quota = None; // Planer authorization is independent of inbound sender quotas.
    let trace = super::logging::QqChatTaskTrace::new(chrono::Local::now());
    let bot_id = zihuan_core::ims_bot_adapter::message_helpers::get_bot_id(adapter);
    let mut prepared = super::core::prepare_current_turn_user_input_from_event(
        &event,
        &bot_id,
        &service.config.bot_name,
        service.config.s3_ref.as_ref(),
    );
    prepared.current_text = input.prompt.clone();
    hydrate_planer_media_into_input(&mut prepared, &input.media, service.config.s3_ref.as_deref());
    let history_key = crate::storage::qq_chat_history_store::conversation_history_key(session_key);
    let history =
        crate::storage::qq_chat_history_store::load_history(&service.config.cache, &history_key)?;
    let turn_id = uuid::Uuid::new_v4().to_string();
    let _trace = zihuan_core::qq_context::TurnTraceGuard::enter(
        &service.config.agent_id,
        session_key,
        &turn_id,
    );
    let dimensions = service.config.qq_chat_config.resolved_emotion_dimensions();
    let mut state = service.config.session_state_store.lock().unwrap().clone();
    state.sync_emotion_dimensions(&dimensions);
    zihuan_core::qq_context::emit_qq_context_event(
        zihuan_core::qq_context::QqChatContextEvent::TurnStarted {
            agent_id: service.config.agent_id.clone(),
            sender_id: session_key.into(),
            sender_name: format!("Planer: {}", input.source_planer_id),
            group_id: event.group_id.clone(),
            group_name: None,
            is_group,
            turn_id: turn_id.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            user_text: input.prompt.clone(),
            quoted: None,
            media: Vec::new(),
            continues_previous: false,
        },
    );
    zihuan_core::qq_context::check_turn_storage()?;
    let _completion = super::turn_workflow::run_turn::TurnCompletion {
        agent: &service.config.agent_id,
        sender: session_key,
        turn: &turn_id,
    };
    let intent = crate::classify_intent::classify_intent_with_trace(
        &service.config.intent_classification_llm,
        service.config.embedding_model.as_ref(),
        &input.prompt,
        Some(&history),
        800,
    );
    let turn_llm = match intent.category {
        crate::classify_intent::IntentCategory::SolveComplexProblem
        | crate::classify_intent::IntentCategory::WriteCode => &service.config.math_programming_llm,
        _ => &service.config.llm,
    };
    super::turn_workflow::normal_turn::run(super::turn_workflow::TurnContext {
        service,
        trace: &trace,
        ctx: &ctx,
        event: &event,
        sender_id: session_key,
        target_id: &input.target_id,
        is_group,
        bot_id: &bot_id,
        continues_previous: false,
        message_rate_limit_warning: None,
        turn_id: &turn_id,
        prepared_input: prepared,
        inference_event: event.clone(),
        current_message: input.prompt.clone(),
        history_key,
        history,
        turn_llm,
        role_config: service.config.qq_chat_config.clone(),
        emotion_dimensions: dimensions,
        turn_session_state: Arc::new(Mutex::new(state)),
    })?;
    let delivery = input.delivery.lock().unwrap().clone();
    let delivery = if delivery.is_null() {
        json!({"success":false,"sent":0,"receipts":[],"error":"Role 未生成回复或回复被审查拒绝"})
    } else {
        delivery
    };
    Ok(delivery)
}

const LOG_PREFIX: &str = "[QqChatAgentService]";

/// Makes Planer-forwarded images part of the turn input: the brain receives the hydrated
/// image parts plus a reference line naming each media_id, so it can attach them to the
/// reply with the `[Image media_id=...]` placeholder the delivery pipeline resolves. A
/// Planer call has no inbound event, so without this the images would silently disappear.
fn hydrate_planer_media_into_input(
    prepared: &mut super::user_input::PreparedCurrentTurnUserInput,
    media: &[PersistedMedia],
    s3_ref: Option<&zihuan_core::storage::S3Ref>,
) {
    for media in media {
        let image = ImageMessage { media: media.clone() };
        let Some(resolved) = resolve_image_message_part(&image, s3_ref, false, LOG_PREFIX) else {
            log::warn!(
                "{LOG_PREFIX} Planer 附件 media_id={} 无法加载，将以文字占位",
                media.media_id
            );
            // A text part, not current_text: the delivery turn builds its task line from
            // the Planer prompt, so only parts are guaranteed to reach the brain.
            prepared.current_parts.push(MessagePart::text(format!(
                "[图片附件 media_id={} 未能加载]",
                media.media_id
            )));
            prepared.multimodal_stats.record_skipped();
            continue;
        };
        prepared.current_parts.push(resolved.part);
        prepared.has_media = true;
        prepared
            .current_image_reference_lines
            .push(format!("Planer 任务附件 media_id={}", media.media_id));
        prepared.multimodal_stats.record_success(resolved.source);
    }
}
