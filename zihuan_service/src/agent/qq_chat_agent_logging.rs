use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::{DateTime, Local};
use log::info;
use serde::Serialize;
use serde_json::Value;

use super::classify_intent::IntentClassificationTrace;
use ims_bot_adapter::models::message::Message;
use zihuan_agent::brain::{BrainObserver, BrainStopReason};
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::OpenAIMessage;

const LOG_PREFIX: &str = "[QqChatAgent]";
const LOG_TEXT_PREVIEW_CHARS: usize = 1_200;
const LOG_TOOL_PREVIEW_CHARS: usize = 600;

#[derive(Debug, Clone)]
struct TracePoint {
    at: DateTime<Local>,
    instant: Instant,
}

impl TracePoint {
    fn now() -> Self {
        Self {
            at: Local::now(),
            instant: Instant::now(),
        }
    }
}

#[derive(Debug, Clone)]
struct ToolCallTrace {
    name: String,
    started_at: TracePoint,
    finished_at: Option<TracePoint>,
}

#[derive(Debug)]
struct QqChatTaskTraceInner {
    ims_adapter_received_at: TracePoint,
    task_created_at: TracePoint,
    intent_finished_at: Option<TracePoint>,
    intent_trace: Option<IntentClassificationTrace>,
    llm_request_started_at: Option<TracePoint>,
    llm_final_result_at: Option<TracePoint>,
    llm_result_parsed_at: Option<TracePoint>,
    reply_send_started_at: Option<TracePoint>,
    reply_send_finished_at: Option<TracePoint>,
    task_finished_at: Option<TracePoint>,
    tool_calls: Vec<ToolCallTrace>,
    tool_call_counts: HashMap<String, usize>,
    history_message_count: Option<usize>,
    history_tokens_estimated: Option<usize>,
    prompt_tokens_estimated: Option<usize>,
    completion_tokens_estimated: Option<usize>,
    total_tokens_estimated: Option<usize>,
    exact_usage_available: bool,
    reply_suppress_send: Option<bool>,
    reply_sent: Option<bool>,
}

#[derive(Clone)]
pub(crate) struct QqChatTaskTrace {
    inner: Arc<Mutex<QqChatTaskTraceInner>>,
}

impl QqChatTaskTrace {
    pub(crate) fn new(task_created_at: DateTime<Local>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(QqChatTaskTraceInner {
                ims_adapter_received_at: TracePoint::now(),
                task_created_at: TracePoint {
                    at: task_created_at,
                    instant: Instant::now(),
                },
                intent_finished_at: None,
                intent_trace: None,
                llm_request_started_at: None,
                llm_final_result_at: None,
                llm_result_parsed_at: None,
                reply_send_started_at: None,
                reply_send_finished_at: None,
                task_finished_at: None,
                tool_calls: Vec::new(),
                tool_call_counts: HashMap::new(),
                history_message_count: None,
                history_tokens_estimated: None,
                prompt_tokens_estimated: None,
                completion_tokens_estimated: None,
                total_tokens_estimated: None,
                exact_usage_available: false,
                reply_suppress_send: None,
                reply_sent: None,
            })),
        }
    }

    pub(crate) fn log_user_message(&self, raw_user_message: &str, current_message: &str) {
        let details = if raw_user_message.trim() == current_message.trim() {
            format!(
                "用户消息: {}",
                truncate_for_log(current_message, LOG_TEXT_PREVIEW_CHARS)
            )
        } else {
            format!(
                "用户消息: raw={} | inference={}",
                truncate_for_log(raw_user_message, LOG_TEXT_PREVIEW_CHARS),
                truncate_for_log(current_message, LOG_TEXT_PREVIEW_CHARS)
            )
        };
        self.log_key_event("收到用户消息", 0, details);
    }

    pub(crate) fn record_intent(&self, trace: IntentClassificationTrace) {
        let finished_at = TracePoint::now();
        let details = format!(
            "意图={} path={} embedding_used={} llm_used={} raw_label={}",
            trace.category.label(),
            trace.path.label(),
            trace.used_embedding,
            trace.used_llm,
            trace.raw_label.as_deref().unwrap_or("<none>")
        );
        self.log_key_event("意图识别完成", trace.total_duration_ms, details);

        let mut inner = self.inner.lock().unwrap();
        inner.intent_finished_at = Some(finished_at);
        inner.intent_trace = Some(trace);
    }

    pub(crate) fn record_history_stats(
        &self,
        history_message_count: usize,
        history_tokens_estimated: usize,
    ) {
        self.log_key_event(
            "历史消息上下文",
            0,
            format!(
                "history_messages={} context_tokens_estimated={}",
                history_message_count, history_tokens_estimated
            ),
        );
        let mut inner = self.inner.lock().unwrap();
        inner.history_message_count = Some(history_message_count);
        inner.history_tokens_estimated = Some(history_tokens_estimated);
    }

    pub(crate) fn record_steer_received(&self, current_message: &str) {
        self.log_key_event(
            "收到插嘴消息",
            0,
            format!(
                "message={}",
                truncate_for_log(current_message, LOG_TEXT_PREVIEW_CHARS)
            ),
        );
    }

    pub(crate) fn record_steer_injected(
        &self,
        steer_count: usize,
        injected_messages: usize,
        accepted_steer_count: usize,
        max_steer_count: usize,
        remaining_queue_len: usize,
        messages: &[OpenAIMessage],
    ) {
        self.log_key_event(
            "插嘴已注入当前对话",
            0,
            format!(
                "steer_count={} injected_messages={} merged={} accepted_steer_count={}/{} remaining_queue_len={} payload={}",
                steer_count,
                injected_messages,
                steer_count > injected_messages,
                accepted_steer_count,
                max_steer_count,
                remaining_queue_len,
                json_for_log(messages, LOG_TEXT_PREVIEW_CHARS)
            ),
        );
    }

    pub(crate) fn record_steer_follow_up(
        &self,
        message_id: i64,
        accepted_steer_count: usize,
        max_steer_count: usize,
        current_message: &str,
    ) {
        self.log_key_event(
            "插嘴触发下一轮",
            0,
            format!(
                "message_id={} accepted_steer_count={}/{} message={}",
                message_id,
                accepted_steer_count,
                max_steer_count,
                truncate_for_log(current_message, LOG_TEXT_PREVIEW_CHARS)
            ),
        );
    }

    pub(crate) fn mark_llm_request_started(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.llm_request_started_at = Some(TracePoint::now());
    }

    pub(crate) fn log_llm_conversation(
        &self,
        conversation: &[OpenAIMessage],
        prompt_tokens_estimated: usize,
    ) {
        self.log_key_event(
            "发送给大模型的消息列表",
            0,
            format!(
                "messages={} prompt_tokens_estimated={} payload={}",
                conversation.len(),
                prompt_tokens_estimated,
                json_for_log(conversation, LOG_TEXT_PREVIEW_CHARS)
            ),
        );
        let mut inner = self.inner.lock().unwrap();
        inner.prompt_tokens_estimated = Some(prompt_tokens_estimated);
    }

    pub(crate) fn record_tool_request(
        &self,
        iteration: usize,
        content: &str,
        tool_calls: &[ToolCalls],
    ) {
        let details = format!(
            "iteration={} assistant_content={} tool_calls={}",
            iteration,
            truncate_for_log(content, LOG_TOOL_PREVIEW_CHARS),
            json_for_log(tool_calls, LOG_TOOL_PREVIEW_CHARS)
        );
        let duration_ms = self
            .inner
            .lock()
            .unwrap()
            .llm_request_started_at
            .as_ref()
            .map(|point| point.instant.elapsed().as_millis())
            .unwrap_or_default();
        self.log_key_event("模型请求工具", duration_ms, details);
    }

    pub(crate) fn record_tool_start(&self, name: &str, arguments: &Value) {
        self.log_key_event(
            &format!("工具调用 {name}"),
            0,
            format!(
                "arguments={}",
                truncate_for_log(&arguments.to_string(), LOG_TOOL_PREVIEW_CHARS)
            ),
        );

        let mut inner = self.inner.lock().unwrap();
        inner.tool_calls.push(ToolCallTrace {
            name: name.to_string(),
            started_at: TracePoint::now(),
            finished_at: None,
        });
        *inner.tool_call_counts.entry(name.to_string()).or_insert(0) += 1;
    }

    pub(crate) fn record_tool_finish(&self, name: &str, result: &str) {
        let finished_at = TracePoint::now();
        let duration_ms = {
            let mut inner = self.inner.lock().unwrap();
            inner
                .tool_calls
                .iter_mut()
                .rev()
                .find(|call| call.name == name && call.finished_at.is_none())
                .map(|call| {
                    call.finished_at = Some(finished_at.clone());
                    finished_at
                        .instant
                        .duration_since(call.started_at.instant)
                        .as_millis()
                })
                .unwrap_or_default()
        };

        self.log_key_event(
            &format!("工具调用结果 {name}"),
            duration_ms,
            format!(
                "result={}",
                truncate_for_log(result, LOG_TOOL_PREVIEW_CHARS)
            ),
        );
    }

    pub(crate) fn record_llm_final_result(
        &self,
        stop_reason: &BrainStopReason,
        brain_output: &[OpenAIMessage],
    ) {
        let now = TracePoint::now();
        let duration_ms = self
            .inner
            .lock()
            .unwrap()
            .llm_request_started_at
            .as_ref()
            .map(|point| now.instant.duration_since(point.instant).as_millis())
            .unwrap_or_default();

        self.log_key_event(
            "大模型返回内容",
            duration_ms,
            format!(
                "stop_reason={stop_reason:?} messages={} payload={}",
                brain_output.len(),
                json_for_log(brain_output, LOG_TEXT_PREVIEW_CHARS)
            ),
        );

        let mut inner = self.inner.lock().unwrap();
        inner.llm_final_result_at = Some(now);
    }

    pub(crate) fn record_llm_result_parsed(&self, final_assistant_text: Option<&str>) {
        let now = TracePoint::now();
        let duration_ms = self
            .inner
            .lock()
            .unwrap()
            .llm_final_result_at
            .as_ref()
            .map(|point| now.instant.duration_since(point.instant).as_millis())
            .unwrap_or_default();

        self.log_key_event(
            "解析大模型返回结果",
            duration_ms,
            format!(
                "assistant_text={}",
                final_assistant_text
                    .map(|text| truncate_for_log(text, LOG_TEXT_PREVIEW_CHARS))
                    .unwrap_or_else(|| "<none>".to_string())
            ),
        );

        let mut inner = self.inner.lock().unwrap();
        inner.llm_result_parsed_at = Some(now);
    }

    pub(crate) fn mark_reply_send_started(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.reply_send_started_at = Some(TracePoint::now());
    }

    pub(crate) fn record_reply_send(
        &self,
        suppress_send: bool,
        reply_sent: bool,
        batches: &[Vec<Message>],
    ) {
        let now = TracePoint::now();
        let duration_ms = self
            .inner
            .lock()
            .unwrap()
            .reply_send_started_at
            .as_ref()
            .map(|point| now.instant.duration_since(point.instant).as_millis())
            .unwrap_or_default();

        self.log_key_event(
            "最终发送的QQ message list",
            duration_ms,
            format!(
                "suppress_send={} actual_sent={} batches={} payload={}",
                suppress_send,
                reply_sent,
                batches.len(),
                json_for_log(batches, LOG_TEXT_PREVIEW_CHARS)
            ),
        );

        let mut inner = self.inner.lock().unwrap();
        inner.reply_send_finished_at = Some(now);
        inner.reply_suppress_send = Some(suppress_send);
        inner.reply_sent = Some(reply_sent);
    }

    pub(crate) fn record_token_usage(&self, completion_tokens_estimated: usize) {
        let mut inner = self.inner.lock().unwrap();
        inner.completion_tokens_estimated = Some(completion_tokens_estimated);
        inner.total_tokens_estimated = inner
            .prompt_tokens_estimated
            .map(|prompt| prompt + completion_tokens_estimated);
    }

    pub(crate) fn log_result_summary(&self, result_summary: &str) {
        self.log_key_event(
            "任务结果",
            0,
            format!(
                "summary={}",
                truncate_for_log(result_summary, LOG_TEXT_PREVIEW_CHARS)
            ),
        );
    }

    pub(crate) fn finish_with_summary(&self) {
        let mut inner = self.inner.lock().unwrap();
        let task_finished_at = TracePoint::now();
        inner.task_finished_at = Some(task_finished_at.clone());

        let mut lines = Vec::new();
        lines.push(format!(
            "ims_bot_adapter消息时间点 {}",
            format_time(&inner.ims_adapter_received_at.at)
        ));
        lines.push(format_timeline_line(
            "创建任务时间点",
            Some(&inner.task_created_at),
            Some(&inner.ims_adapter_received_at),
        ));
        lines.push(format_timeline_line(
            "意图分类识别时间点",
            inner.intent_finished_at.as_ref(),
            Some(&inner.task_created_at),
        ));

        if let Some(intent_trace) = inner.intent_trace.as_ref() {
            if intent_trace.used_embedding {
                let embedding_duration = intent_trace.embedding_duration_ms.unwrap_or_default();
                let embedding_time = inner
                    .intent_finished_at
                    .as_ref()
                    .map(|finished| finished.at.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                lines.push(format!(
                    "text embedding推理时间点 {} [耗时 {} ms]",
                    embedding_time, embedding_duration
                ));
            } else {
                lines.push("text embedding推理时间点 未触发".to_string());
            }
        } else {
            lines.push("text embedding推理时间点 未触发".to_string());
        }

        lines.push(format_timeline_line(
            "开始组件system prompt等message发送给大模型的时间点",
            inner.llm_request_started_at.as_ref(),
            inner.intent_finished_at.as_ref(),
        ));

        for (index, tool_call) in inner.tool_calls.iter().enumerate() {
            lines.push(format_timeline_line(
                &format!("工具{}调用时间点 {}", index + 1, tool_call.name),
                Some(&tool_call.started_at),
                inner.llm_request_started_at.as_ref(),
            ));
            if let Some(finished_at) = tool_call.finished_at.as_ref() {
                lines.push(format_timeline_line(
                    &format!("工具{}完成时间点 {}", index + 1, tool_call.name),
                    Some(finished_at),
                    Some(&tool_call.started_at),
                ));
            }
        }

        lines.push(format_timeline_line(
            "大模型最终结果时间点",
            inner.llm_final_result_at.as_ref(),
            inner.llm_request_started_at.as_ref(),
        ));
        lines.push(format_timeline_line(
            "解析大模型返回结果时间点",
            inner.llm_result_parsed_at.as_ref(),
            inner.llm_final_result_at.as_ref(),
        ));
        lines.push(format_timeline_line(
            "发送回文时间点",
            inner.reply_send_finished_at.as_ref(),
            inner
                .reply_send_started_at
                .as_ref()
                .or(inner.llm_result_parsed_at.as_ref()),
        ));
        lines.push(format_timeline_line(
            "任务结束时间点",
            inner.task_finished_at.as_ref(),
            inner
                .reply_send_finished_at
                .as_ref()
                .or(inner.llm_result_parsed_at.as_ref())
                .or(inner.intent_finished_at.as_ref()),
        ));
        lines.push("---".to_string());

        let mut tool_counts: Vec<_> = inner.tool_call_counts.iter().collect();
        tool_counts.sort_by(|left, right| left.0.cmp(right.0));
        let tool_count_text = if tool_counts.is_empty() {
            "none".to_string()
        } else {
            tool_counts
                .into_iter()
                .map(|(name, count)| format!("{name}x{count}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        lines.push(format!("工具调用次数: [{tool_count_text}]"));
        lines.push(format!(
            "本次推理token消耗 estimated_prompt_tokens={} estimated_completion_tokens={} estimated_total_tokens={} exact_usage_available={}",
            inner
                .prompt_tokens_estimated
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            inner
                .completion_tokens_estimated
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            inner
                .total_tokens_estimated
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            inner.exact_usage_available
        ));
        lines.push(format!(
            "历史消息队列数量={}，token总数={}",
            inner
                .history_message_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string()),
            inner
                .history_tokens_estimated
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string())
        ));

        info!("{LOG_PREFIX}\n{}", lines.join("\n"));
    }

    fn log_key_event(&self, title: &str, duration_ms: u128, details: impl AsRef<str>) {
        info!(
            "{LOG_PREFIX} {title} [耗时 {duration_ms} ms] {}",
            details.as_ref()
        );
    }
}

pub(crate) struct QqChatBrainObserver {
    pub(crate) trace: QqChatTaskTrace,
}

impl BrainObserver for QqChatBrainObserver {
    fn on_assistant_tool_request(&self, iteration: usize, content: &str, tool_calls: &[ToolCalls]) {
        self.trace
            .record_tool_request(iteration, content, tool_calls);
    }

    fn on_tool_start(&self, name: &str, _call_id: &str, arguments: &Value) {
        self.trace.record_tool_start(name, arguments);
    }

    fn on_tool_finish(&self, name: &str, _call_id: &str, result: &str) {
        self.trace.record_tool_finish(name, result);
    }
}

fn format_time(time: &DateTime<Local>) -> String {
    time.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

fn format_timeline_line(
    label: &str,
    point: Option<&TracePoint>,
    previous: Option<&TracePoint>,
) -> String {
    match point {
        Some(point) => {
            let duration_ms = previous
                .map(|previous| point.instant.duration_since(previous.instant).as_millis())
                .unwrap_or_default();
            format!(
                "{label} {} [耗时 {} ms]",
                format_time(&point.at),
                duration_ms
            )
        }
        None => format!("{label} 未触发"),
    }
}

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...(truncated,total_chars={total_chars})")
}

fn json_for_log<T: Serialize + ?Sized>(value: &T, max_chars: usize) -> String {
    match serde_json::to_string(value) {
        Ok(json) => truncate_for_log(&json, max_chars),
        Err(err) => format!("<serialize failed: {err}>"),
    }
}
