use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use storage_handler::redis::{blpop_value, rpush_value};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinSet;
use tokio::time::sleep;
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::MessageEvent;
use zihuan_graph_engine::data_value::RedisConfig;

use ims_bot_adapter::adapter::SharedBotAdapter;

use super::qq_chat_agent_core::QqChatAgentService;

const DEFAULT_CONSUMER_COUNT: usize = 8;
const REDIS_DEQUEUE_TIMEOUT_SECS: usize = 1;
const REDIS_RETRY_DELAY_MS: u64 = 250;
const REDIS_QUEUE_PREFIX: &str = "qq_chat_agent:inbox";

#[derive(Debug, Clone)]
pub enum QqChatAgentSupervisorEvent {
    AdapterFinished {
        success: bool,
        error_msg: Option<String>,
    },
    RedisConsumerFinished,
    MemoryConsumerFinished,
}

#[derive(Debug, Clone)]
pub enum QqChatAgentInboxBackend {
    Redis,
    Memory,
}

#[derive(Clone)]
pub struct QqChatAgentInbox {
    inner: Arc<QqChatAgentInboxInner>,
}

struct QqChatAgentInboxInner {
    service: Arc<QqChatAgentService>,
    adapter: SharedBotAdapter,
    redis_ref: Option<Arc<RedisConfig>>,
    redis_queue_key: String,
    memory_queue: Arc<MemoryInboxQueue>,
    consumer_count: usize,
    shutdown: Arc<InboxShutdown>,
}

#[derive(Clone)]
struct QqChatAgentInboxItem {
    event: MessageEvent,
    adapter: SharedBotAdapter,
    time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredQqChatAgentInboxItem {
    event: MessageEvent,
    time: String,
}

struct MemoryInboxQueue {
    queue: Mutex<VecDeque<QqChatAgentInboxItem>>,
    notify: Notify,
}

struct InboxShutdown {
    closing: AtomicBool,
    notify: Notify,
}

impl MemoryInboxQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    async fn push(&self, item: QqChatAgentInboxItem) {
        let mut guard = self.queue.lock().await;
        guard.push_back(item);
        drop(guard);
        self.notify.notify_one();
    }

    async fn pop(&self) -> QqChatAgentInboxItem {
        loop {
            if let Some(item) = {
                let mut guard = self.queue.lock().await;
                guard.pop_front()
            } {
                return item;
            }
            self.notify.notified().await;
        }
    }
}

impl InboxShutdown {
    fn new() -> Self {
        Self {
            closing: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    fn is_closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    fn request_shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl QqChatAgentInbox {
    pub fn new(
        service: Arc<QqChatAgentService>,
        adapter: SharedBotAdapter,
        redis_ref: Option<Arc<RedisConfig>>,
        agent_id: &str,
        consumer_count: Option<usize>,
    ) -> Self {
        let consumer_count = consumer_count.unwrap_or(DEFAULT_CONSUMER_COUNT).max(1);
        Self {
            inner: Arc::new(QqChatAgentInboxInner {
                service,
                adapter,
                redis_ref,
                redis_queue_key: format!("{REDIS_QUEUE_PREFIX}:{agent_id}"),
                memory_queue: Arc::new(MemoryInboxQueue::new()),
                consumer_count,
                shutdown: Arc::new(InboxShutdown::new()),
            }),
        }
    }

    pub fn request_shutdown(&self) {
        self.inner.shutdown.request_shutdown();
    }

    pub async fn enqueue(
        &self,
        event: MessageEvent,
        time: String,
    ) -> Result<QqChatAgentInboxBackend> {
        let item = QqChatAgentInboxItem {
            event,
            adapter: Arc::clone(&self.inner.adapter),
            time,
        };
        let message_id = item.event.message_id;
        let sender_id = item.event.sender.user_id;

        if self.inner.redis_ref.is_some() {
            match self.enqueue_to_redis(&item).await {
                Ok(()) => {
                    info!(
                        "[service][qq_agent][inbox] enqueued message_id={} sender={} backend=redis",
                        message_id, sender_id
                    );
                    return Ok(QqChatAgentInboxBackend::Redis);
                }
                Err(err) => {
                    warn!(
                        "[service][qq_agent][inbox] redis enqueue failed for message_id={} sender={}: {}, falling back to memory",
                        message_id, sender_id, err
                    );
                }
            }
        }

        self.inner.memory_queue.push(item).await;
        info!(
            "[service][qq_agent][inbox] enqueued message_id={} sender={} backend=memory",
            message_id, sender_id
        );
        Ok(QqChatAgentInboxBackend::Memory)
    }

    pub fn spawn_consumers(&self, tasks: &mut JoinSet<QqChatAgentSupervisorEvent>) {
        if self.inner.redis_ref.is_some() {
            for consumer_idx in 0..self.inner.consumer_count {
                let inbox = self.clone();
                tasks.spawn(async move {
                    inbox.run_redis_consumer(consumer_idx).await;
                    QqChatAgentSupervisorEvent::RedisConsumerFinished
                });
            }
        }

        for consumer_idx in 0..self.inner.consumer_count {
            let inbox = self.clone();
            tasks.spawn(async move {
                inbox.run_memory_consumer(consumer_idx).await;
                QqChatAgentSupervisorEvent::MemoryConsumerFinished
            });
        }
    }

    async fn enqueue_to_redis(&self, item: &QqChatAgentInboxItem) -> Result<()> {
        let Some(redis_ref) = self.inner.redis_ref.as_ref() else {
            return Ok(());
        };
        let stored = StoredQqChatAgentInboxItem {
            event: item.event.clone(),
            time: item.time.clone(),
        };
        let payload = serde_json::to_string(&stored)?;
        rpush_value(redis_ref, &self.inner.redis_queue_key, &payload).await?;
        Ok(())
    }

    async fn run_redis_consumer(&self, consumer_idx: usize) {
        loop {
            if self.inner.shutdown.is_closing() {
                break;
            }
            match self.dequeue_from_redis().await {
                Ok(Some(item)) => {
                    self.process_item(item).await;
                }
                Ok(None) => continue,
                Err(err) => {
                    warn!(
                        "[service][qq_agent][inbox][redis:{}] dequeue failed: {}",
                        consumer_idx, err
                    );
                    sleep(Duration::from_millis(REDIS_RETRY_DELAY_MS)).await;
                }
            }
        }
    }

    async fn run_memory_consumer(&self, consumer_idx: usize) {
        loop {
            let item = tokio::select! {
                _ = self.inner.shutdown.notify.notified() => {
                    if self.inner.shutdown.is_closing() {
                        break;
                    }
                    continue;
                }
                item = self.inner.memory_queue.pop() => item,
            };
            info!(
                "[service][qq_agent][inbox][memory:{}] dequeued message_id={} sender={}",
                consumer_idx, item.event.message_id, item.event.sender.user_id
            );
            self.process_item(item).await;
        }
    }

    async fn dequeue_from_redis(&self) -> Result<Option<QqChatAgentInboxItem>> {
        let Some(redis_ref) = self.inner.redis_ref.as_ref() else {
            return Ok(None);
        };
        let result = blpop_value(
            redis_ref,
            &self.inner.redis_queue_key,
            REDIS_DEQUEUE_TIMEOUT_SECS,
        )
        .await?;
        let Some((_, payload)) = result else {
            return Ok(None);
        };
        let stored: StoredQqChatAgentInboxItem = serde_json::from_str(&payload)?;
        Ok(Some(QqChatAgentInboxItem {
            event: stored.event,
            adapter: Arc::clone(&self.inner.adapter),
            time: stored.time,
        }))
    }

    async fn process_item(&self, item: QqChatAgentInboxItem) {
        let service = Arc::clone(&self.inner.service);
        let event = item.event;
        let adapter = item.adapter;
        let time = item.time;
        let message_id = event.message_id;
        let sender_id = event.sender.user_id;

        let result =
            tokio::task::spawn_blocking(move || service.handle_event(&event, &adapter, &time))
                .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                error!(
                    "[service][qq_agent][inbox] failed to handle message_id={} sender={}: {}",
                    message_id, sender_id, err
                );
            }
            Err(err) => {
                error!(
                    "[service][qq_agent][inbox] blocking worker failed for message_id={} sender={}: {}",
                    message_id, sender_id, err
                );
            }
        }
    }
}
