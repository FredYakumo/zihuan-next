use std::sync::{Arc, Mutex};

use zihuan_core::command::Effect;
use zihuan_core::command::{
    execute_builtin, CmdState, CommandChannel, CommandContext, CommandRuntime,
    ExecutionSnapshot, InputGate, Invocation, ResumeAction, Step, StepControl, StepRun,
};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};

use crate::qq_chat::language_style_store::LanguageStyleScope;
use crate::qq_chat::logging::QqChatTaskTrace;
use crate::qq_chat::model::QqChatAgentServiceContext;
use crate::qq_chat::msg_send::send_direct_text_reply;
use crate::qq_chat::privilege_gate::{AuthCommandOutcome, QqPrivilegedCommand};
use crate::qq_chat::privilege_store;
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;

/// QQ channel command runtime — one instance per claimed turn.
///
/// Implements the `ims://*` step ops and renders [`Effect`]s to QQ chat. A
/// pause (authorization gate) is persisted by creating the privilege-auth
/// record with the serialized [`ExecutionSnapshot`], so a later `/auth`
/// resumes the exact remaining steps.
pub(crate) struct QqChatCommandRuntime<'a> {
    pub(crate) trace: &'a QqChatTaskTrace,
    pub(crate) ctx: &'a QqChatAgentServiceContext<'a>,
    pub(crate) agent_id: &'a str,
    pub(crate) event: zihuan_core::ims_bot_adapter::models::MessageEvent,
    pub(crate) inference_event: zihuan_core::ims_bot_adapter::models::MessageEvent,
    pub(crate) sender_id: &'a str,
    pub(crate) target_id: &'a str,
    pub(crate) bot_id: &'a str,
    pub(crate) is_group: bool,
    pub(crate) session_state_store: &'a Arc<Mutex<QqChatSessionState>>,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
}

impl<'a> QqChatCommandRuntime<'a> {
    fn rdb(&self) -> Result<&RelationalDbConnection> {
        self.ctx.rdb_pool.ok_or_else(|| {
            Error::ValidationError("当前未配置关系数据库，无法完成特权授权。".to_string())
        })
    }

    fn group_name(&self) -> Option<&str> {
        self.event.group_name.as_deref()
    }

    /// Target / group / is_group used for sends. Defaults to the turn's own
    /// channel; resumed legacy commands carry their own context instead.
    fn channel_target(&self, inv: &Invocation) -> (String, bool) {
        match &inv.ctx.channel {
            CommandChannel::QqChat { target_id, is_group, .. } => {
                (target_id.clone(), *is_group)
            }
            _ => (self.target_id.to_string(), self.is_group),
        }
    }
}

impl<'a> CommandRuntime for QqChatCommandRuntime<'a> {
    fn run_step(
        &mut self,
        step: &Step,
        inv: &Invocation,
        state: &mut CmdState,
    ) -> Result<StepRun> {
        if let Some(op) = step.op.strip_prefix("builtin://") {
            return execute_builtin_step(op, inv);
        }
        match step.op.as_str() {
            "ims://privilege/active" => {
                let connection = self.rdb()?;
                if privilege_store::has_active_privilege_blocking(
                    connection,
                    self.agent_id,
                    self.sender_id,
                )? {
                    return Ok(StepRun::continue_with(Vec::new()));
                }
                let purpose = step
                    .params
                    .get("purpose")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&step.op);
                let prompt = crate::qq_chat::privilege_gate::render_privilege_auth_prompt(purpose);
                Ok(StepRun {
                    effects: vec![Effect::Text(prompt)],
                    control: StepControl::RequireInput(InputGate::NeedsAuth {
                        purpose: purpose.to_string(),
                    }),
                })
            }
            "ims://emotion/execute" => {
                let command = match step.params.get("command").and_then(|v| v.as_str()) {
                    Some("adjust_emotion") => QqPrivilegedCommand::AdjustEmotion,
                    _ => QqPrivilegedCommand::Emotion,
                };
                let reply = execute_emotion_impl(
                    self.session_state_store,
                    &self.emotion_dimensions,
                    command,
                    &inv.args,
                );
                Ok(StepRun::stop_with(vec![Effect::Text(reply)]))
            }
            "ims://style/prepare_waiting_task" => self.prepare_waiting_task(state, step),
            "ims://style/start" => self.start_style_learning(inv, state, step),
            "ims://auth/verify" => self.verify_auth(inv),
            _ => Err(Error::ValidationError(format!(
                "命令执行器不支持操作 '{}'",
                step.op
            ))),
        }
    }

    fn apply_effect(&mut self, effect: &Effect) -> Result<()> {
        // apply_effect receives no invocation; use the runtime's own turn
        // channel (rendering during a normal turn).
        match effect {
            Effect::Text(text) | Effect::Notice(text) => {
                send_direct_text_reply(
                    self.trace,
                    self.ctx.adapter,
                    self.target_id,
                    self.ctx.rdb_pool,
                    self.group_name(),
                    self.ctx.bot_name,
                    self.bot_id,
                    text,
                    self.is_group,
                    self.sender_id,
                    &self.inference_event.sender.nickname,
                    &self.inference_event.sender.card,
                    self.ctx.max_message_length,
                    self.ctx.reply_batch_builder,
                )?;
            }
            Effect::Forward(content) => {
                let send_ctx = crate::qq_chat::msg_send::QqChatServiceSendContext {
                    adapter: self.ctx.adapter,
                    target_id: self.target_id,
                    is_group: self.is_group,
                    group_name: self.group_name(),
                    bot_id: self.bot_id,
                    bot_name: self.ctx.bot_name,
                    mention_target_id: None,
                    persistence:
                        crate::storage::qq_chat_session_store::build_outbound_persistence(
                            self.ctx.rdb_pool,
                            self.group_name(),
                            self.ctx.bot_name,
                        ),
                    max_text_chars: self.ctx.max_message_length,
                };
                crate::qq_chat::msg_send::send_forward_content(&send_ctx, content)?;
            }
            Effect::StartNewConversation => {
                crate::storage::qq_chat_history_store::clear_history(
                    self.ctx.cache,
                    self.sender_id,
                )?;
            }
            // The engine folds SetContext into the shared CmdState before
            // delivery, so the QQ runtime never receives one.
            Effect::SetContext { .. } => {}
        }
        Ok(())
    }

    fn persist_pause(&mut self, snapshot: &ExecutionSnapshot) -> Result<()> {
        let connection = self.rdb()?;
        let InputGate::NeedsAuth { purpose } = &snapshot.gate;
        let state_task_id = snapshot.state.get_str("style_waiting_task_id").map(str::to_string);
        let (target_id, group_id, is_group) = match &snapshot.invocation.ctx.channel {
            CommandChannel::QqChat { target_id, group_id, is_group, .. } => {
                (target_id.clone(), *group_id, *is_group)
            }
            _ => (self.target_id.to_string(), self.event.group_id, self.is_group),
        };
        privilege_store::create_privilege_auth_blocking(
            connection,
            self.agent_id,
            self.sender_id,
            purpose,
            state_task_id.as_deref(),
            Some(target_id.as_str()),
            group_id,
            is_group,
            &snapshot.invocation.args,
        )?;
        let json = serde_json::to_string(snapshot)
            .map_err(|e| Error::ValidationError(format!("命令挂起快照序列化失败: {e}")))?;
        privilege_store::update_snapshot_blocking(
            connection,
            self.agent_id,
            self.sender_id,
            Some(json),
        )?;
        Ok(())
    }
}

impl<'a> QqChatCommandRuntime<'a> {
    fn prepare_waiting_task(&mut self, state: &mut CmdState, step: &Step) -> Result<StepRun> {
        let Some(runtime) = self.ctx.task_runtime.clone() else {
            return Err(Error::ValidationError("task runtime is not available".to_string()));
        };
        // Prepared before the privilege gate; the spec's step params carry the
        // human-readable task name (group vs global learning).
        let task_name = step
            .params
            .get("display")
            .and_then(|v| v.as_str())
            .unwrap_or("学习全局语言风格");
        let waiting = runtime.start_waiting_auth_task(zihuan_core::task_context::AgentTaskRequest {
            task_name: task_name.to_string(),
            agent_id: self.agent_id.to_string(),
            agent_name: self.ctx.bot_name.to_string(),
            user_ip: None,
            owner_id: Some(self.sender_id.to_string()),
            task_db_connection_id: self.ctx.task_db_connection_id.clone(),
        });
        state.insert("style_waiting_task_id", serde_json::json!(waiting.task_id));
        Ok(StepRun::continue_with(Vec::new()))
    }

    fn start_style_learning(
        &mut self,
        inv: &Invocation,
        state: &mut CmdState,
        step: &Step,
    ) -> Result<StepRun> {
        let scope = match step.params.get("scope").and_then(|v| v.as_str()) {
            Some("group") => {
                let is_group =
                    matches!(inv.ctx.channel, CommandChannel::QqChat { is_group: true, .. });
                if !is_group {
                    return Ok(StepRun::stop_with(vec![Effect::Text(
                        "当前不是群聊，无法学习群聊语言风格。".to_string(),
                    )]));
                }
                let target_id = match &inv.ctx.channel {
                    CommandChannel::QqChat { target_id, .. } => target_id.clone(),
                    _ => self.target_id.to_string(),
                };
                LanguageStyleScope::Group { group_id: target_id }
            }
            _ => LanguageStyleScope::Global,
        };
        let Some(task_runtime) = self.ctx.task_runtime.clone() else {
            return Err(Error::ValidationError("task runtime is not available".to_string()));
        };
        let task_id = state
            .get_str("style_waiting_task_id")
            .unwrap_or_default()
            .to_string();
        if task_id.is_empty() {
            return Err(Error::ValidationError(
                "pending style-learning task id is missing".to_string(),
            ));
        }
        let Some(connection) = self.ctx.rdb_pool.cloned() else {
            return Err(Error::ValidationError(
                "当前未配置关系数据库，无法执行语言风格学习。".to_string(),
            ));
        };
        let owned = crate::qq_chat::style_learner::OwnedStyleLearningTaskContext {
            adapter: self.ctx.adapter.clone(),
            bot_name: self.ctx.bot_name.to_string(),
            natural_language_reply_llm: Arc::clone(self.ctx.natural_language_reply_llm),
            intent_classification_llm: Arc::clone(self.ctx.intent_classification_llm),
            rdb_pool: connection,
            max_message_length: self.ctx.max_message_length,
            reply_batch_builder: self.ctx.reply_batch_builder.cloned(),
            resolved_language_style_prompt: self
                .ctx
                .resolved_language_style
                .as_ref()
                .map(|item| item.style_prompt.clone()),
        };
        let event = self.event.clone();
        let inference_event = self.inference_event.clone();
        let sender_id = self.sender_id.to_string();
        let (target_id, is_group) = self.channel_target(inv);
        let bot_id = self.bot_id.to_string();
        let trace = self.trace.clone();
        let task_runtime_for_runner = task_runtime.clone();
        let resumed = task_runtime.resume_waiting_auth_task(
            &task_id,
            Box::new(move |task_handle| {
                let input = crate::qq_chat::style_learner::StyleLearningResumeInput {
                    event,
                    inference_event,
                    sender_id,
                    target_id,
                    bot_id,
                    is_group,
                    scope,
                };
                crate::qq_chat::style_learner::execute_style_learning_task(
                    owned,
                    input,
                    trace,
                    task_handle,
                    task_runtime_for_runner,
                );
            }),
        );
        if !resumed {
            return Err(Error::ValidationError(
                "等待授权的风格学习任务无法恢复".to_string(),
            ));
        }
        Ok(StepRun::stop_with(Vec::new()))
    }

    fn verify_auth(&mut self, inv: &Invocation) -> Result<StepRun> {
        let connection = self.rdb()?;
        let key = inv.args.first().map(String::as_str).unwrap_or("");
        if key.trim().is_empty() {
            return Ok(StepRun::stop_with(vec![Effect::Text(
                crate::qq_chat::privilege_gate::render_auth_usage_prompt(),
            )]));
        }
        match crate::qq_chat::privilege_gate::handle_auth_command(
            connection,
            self.agent_id,
            self.sender_id,
            key,
        )? {
            AuthCommandOutcome::Reply(reply) => Ok(StepRun::stop_with(vec![Effect::Text(reply)])),
            AuthCommandOutcome::Resume { message, pending } => {
                let effects = vec![Effect::Text(message)];
                let snapshot_json = privilege_store::load_snapshot_json_blocking(
                    connection,
                    self.agent_id,
                    self.sender_id,
                )?;
                if let Some(json) = snapshot_json {
                    let snapshot: ExecutionSnapshot = serde_json::from_str(&json).map_err(
                        |e| Error::ValidationError(format!("命令挂起快照反序列化失败: {e}")),
                    )?;
                    return Ok(StepRun {
                        effects,
                        control: StepControl::Resume(ResumeAction::Snapshot(snapshot)),
                    });
                }
                // Legacy rows: purpose-based resume, reconstructing the original
                // context from the stored pending fields.
                let purpose = pending.command.command_name().to_string();
                let target_id = pending
                    .pending_target_id
                    .clone()
                    .unwrap_or_else(|| self.target_id.to_string());
                let ctx = CommandContext {
                    agent_type: "qq_chat".to_string(),
                    agent_id: self.agent_id.to_string(),
                    caller_id: self.sender_id.to_string(),
                    channel: CommandChannel::QqChat {
                        sender_id: self.sender_id.to_string(),
                        is_group: pending.pending_is_group,
                        group_id: pending.pending_group_id,
                        target_id,
                    },
                };
                let mut initial_state = CmdState::default();
                if let Some(task_id) = &pending.pending_task_id {
                    initial_state.insert("style_waiting_task_id", serde_json::json!(task_id));
                }
                Ok(StepRun {
                    effects,
                    control: StepControl::Resume(ResumeAction::Command {
                        command: purpose,
                        args: pending.pending_args,
                        ctx_override: Some(ctx),
                        initial_state,
                    }),
                })
            }
        }
    }
}

fn execute_builtin_step(op: &str, inv: &Invocation) -> Result<StepRun> {
    let effects = match execute_builtin(op, &inv.args, &inv.ctx.caller_id) {
        Some(Ok(effects)) => effects,
        Some(Err(err)) => return Err(err),
        None => return Err(Error::ValidationError(format!("builtin 操作不存在: {op}"))),
    };
    Ok(StepRun::stop_with(effects))
}

/// Execute a command through the state machine and render its effects now
/// (used by the busy-session steer bypass path). Returns `Ok(true)` when the
/// command produced output (and therefore consumed the message), `Ok(false)`
/// when it was not a bypassable command.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_command_effects_now(
    trace: &QqChatTaskTrace,
    ctx: &QqChatAgentServiceContext<'_>,
    agent_id: &str,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    sender_id: &str,
    target_id: &str,
    bot_id: &str,
    is_group: bool,
    raw_user_message: &str,
) -> Result<bool> {
    let Some(registry) = zihuan_core::command::global_command_registry() else {
        return Ok(false);
    };
    if !raw_user_message.trim_start().starts_with('/') {
        return Ok(false);
    }
    let cmd_ctx = CommandContext {
        agent_type: "qq_chat".to_string(),
        agent_id: agent_id.to_string(),
        caller_id: sender_id.to_string(),
        channel: CommandChannel::QqChat {
            sender_id: sender_id.to_string(),
            is_group,
            group_id: inference_event.group_id,
            target_id: target_id.to_string(),
        },
    };
    let Some((spec, parsed)) = registry.spec_for(&cmd_ctx, raw_user_message) else {
        return Ok(false);
    };
    let permission = registry.check_permission(&cmd_ctx, raw_user_message);
    if permission.matched && !permission.allowed {
        let mut rt = make_runtime(
            trace,
            ctx,
            agent_id,
            event,
            inference_event,
            sender_id,
            target_id,
            bot_id,
            is_group,
        );
        zihuan_core::command::CommandRuntime::apply_effect(
            &mut rt,
            &Effect::Text("你没有权限使用此命令。".to_string()),
        )?;
        return Ok(true);
    }
    let emotion_dimensions =
        crate::qq_chat::resources::current_qq_chat_role_service_config()?
            .resolved_emotion_dimensions();
    let mut rt = make_runtime_with_emotion(
        trace,
        ctx,
        agent_id,
        event,
        inference_event,
        sender_id,
        target_id,
        bot_id,
        is_group,
        &emotion_dimensions,
    );
    let invocation = Invocation {
        ctx: cmd_ctx,
        args: parsed.args,
        passthrough: parsed.passthrough_text,
        resumed: false,
    };
    let result = zihuan_core::command::execute_command(&mut rt, spec, invocation)?;
    // Busy-bypass only handles command-only turns; any passthrough means the
    // command cannot bypass the steer queue after all.
    if result.passthrough.is_some() {
        return Ok(false);
    }
    let mut delivered = false;
    for effect in &result.effects {
        zihuan_core::command::CommandRuntime::apply_effect(&mut rt, effect)?;
        delivered = true;
    }
    Ok(delivered || result.waiting.is_some())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_runtime<'a>(
    trace: &'a QqChatTaskTrace,
    ctx: &'a QqChatAgentServiceContext<'a>,
    agent_id: &'a str,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    sender_id: &'a str,
    target_id: &'a str,
    bot_id: &'a str,
    is_group: bool,
) -> QqChatCommandRuntime<'a> {
    make_runtime_with_emotion(
        trace,
        ctx,
        agent_id,
        event,
        inference_event,
        sender_id,
        target_id,
        bot_id,
        is_group,
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn make_runtime_with_emotion<'a>(
    trace: &'a QqChatTaskTrace,
    ctx: &'a QqChatAgentServiceContext<'a>,
    agent_id: &'a str,
    event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
    sender_id: &'a str,
    target_id: &'a str,
    bot_id: &'a str,
    is_group: bool,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> QqChatCommandRuntime<'a> {
    QqChatCommandRuntime {
        trace,
        ctx,
        agent_id,
        event: event.clone(),
        inference_event: inference_event.clone(),
        sender_id,
        target_id,
        bot_id,
        is_group,
        session_state_store: ctx.session_state_store,
        emotion_dimensions: emotion_dimensions.to_vec(),
    }
}


fn execute_emotion_impl(
    session_state_store: &Arc<Mutex<QqChatSessionState>>,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    command: QqPrivilegedCommand,
    args: &[String],
) -> String {
    match command {
        QqPrivilegedCommand::Emotion => {
            if !args.is_empty() {
                return "用法: /emotion".to_string();
            }
            let mut session_state = session_state_store.lock().unwrap();
            session_state.sync_emotion_dimensions(emotion_dimensions);
            format!(
                "当前 Agent 情绪维度：\n{}",
                crate::agent::emotion::utils::emotion_dimensions_text(
                    &session_state,
                    emotion_dimensions
                )
            )
        }
        QqPrivilegedCommand::AdjustEmotion => {
            if args.len() != 2 {
                return "用法: /adjust_emotion <维度名> <increase|decrease>".to_string();
            }
            let dimension_name = args[0].trim();
            if !emotion_dimensions
                .iter()
                .any(|dimension| dimension.name.trim() == dimension_name)
            {
                return format!("不支持的情绪维度「{dimension_name}」。");
            }
            let direction = match args[1].to_ascii_lowercase().as_str() {
                "increase" => crate::qq_session_state::EmotionAdjustmentDirection::Increase,
                "decrease" => crate::qq_session_state::EmotionAdjustmentDirection::Decrease,
                _ => return "方向仅支持 increase 或 decrease。".to_string(),
            };
            let direction_label = match direction {
                crate::qq_session_state::EmotionAdjustmentDirection::Increase => "增加",
                crate::qq_session_state::EmotionAdjustmentDirection::Decrease => "降低",
            };
            let current_value =
                match session_state_store.lock().unwrap().apply_emotion_adjustment(
                    emotion_dimensions,
                    dimension_name,
                    direction,
                ) {
                    Ok(value) => value,
                    Err(error) => return format!("调整情绪维度失败：{error}"),
                };
            format!(
                "已{direction_label} Agent 情绪维度「{dimension_name}」，当前值：{current_value}"
            )
        }
        QqPrivilegedCommand::LearnGlobalStyle | QqPrivilegedCommand::LearnGroupStyle => {
            "不支持的情绪命令。".to_string()
        }
    }
}
