use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Mutex;
use std::thread;

use log::warn;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;

use super::local_candle_embedding::LocalCandleEmbeddingModel;

const DEFAULT_EMBEDDING_QUEUE_CAPACITY: usize = 1024;

struct EmbeddingRequest {
    texts: Vec<String>,
    response: SyncSender<Result<Vec<Vec<f32>>>>,
}

struct EmbeddingWorkerHandle {
    sender: SyncSender<EmbeddingRequest>,
}

pub struct QueuedEmbeddingModel {
    model_name: String,
    queue_capacity: usize,
    worker: Mutex<Option<EmbeddingWorkerHandle>>,
}

impl QueuedEmbeddingModel {
    pub fn new(model_name: impl Into<String>) -> Result<Self> {
        let model_name = model_name.into();
        let model = Self {
            model_name,
            queue_capacity: DEFAULT_EMBEDDING_QUEUE_CAPACITY,
            worker: Mutex::new(None),
        };
        model.ensure_worker()?;
        Ok(model)
    }

    fn ensure_worker(&self) -> Result<SyncSender<EmbeddingRequest>> {
        let mut guard = self
            .worker
            .lock()
            .map_err(|_| Error::StringError("embedding worker state lock poisoned".to_string()))?;

        if guard.is_none() {
            *guard = Some(EmbeddingWorkerHandle {
                sender: self.spawn_worker()?,
            });
        }

        Ok(guard
            .as_ref()
            .expect("worker initialized above")
            .sender
            .clone())
    }

    fn reset_worker(&self) {
        if let Ok(mut guard) = self.worker.lock() {
            *guard = None;
        }
    }

    fn spawn_worker(&self) -> Result<SyncSender<EmbeddingRequest>> {
        let model_name = self.model_name.clone();
        let mut model = LocalCandleEmbeddingModel::load(&model_name)?;
        let (sender, receiver) = mpsc::sync_channel::<EmbeddingRequest>(self.queue_capacity);
        let worker_name = format!("embedding-worker-{}", model_name.replace('/', "_"));
        let worker_name_for_thread = worker_name.clone();

        thread::Builder::new()
            .name(worker_name.clone())
            .spawn(move || run_embedding_worker(&worker_name_for_thread, &mut model, receiver))
            .map_err(|err| {
                Error::StringError(format!(
                    "failed to spawn embedding worker '{}' : {}",
                    worker_name, err
                ))
            })?;

        Ok(sender)
    }

    fn request_embeddings(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        for attempt in 1..=2 {
            let sender = self.ensure_worker()?;
            let (response_tx, response_rx) = mpsc::sync_channel::<Result<Vec<Vec<f32>>>>(1);
            let request = EmbeddingRequest {
                texts: texts.clone(),
                response: response_tx,
            };

            match sender.send(request) {
                Ok(()) => match response_rx.recv() {
                    Ok(result) => return result,
                    Err(err) if attempt < 2 => {
                        warn!(
                            "Queued embedding worker stopped before replying for model '{}' (attempt {}): {}. Restarting worker.",
                            self.model_name, attempt, err
                        );
                        self.reset_worker();
                    }
                    Err(err) => {
                        return Err(Error::StringError(format!(
                            "queued embedding worker stopped before replying for model '{}': {}",
                            self.model_name, err
                        )));
                    }
                },
                Err(err) if attempt < 2 => {
                    warn!(
                        "Failed to enqueue embedding request for model '{}' (attempt {}): {}. Restarting worker.",
                        self.model_name, attempt, err
                    );
                    self.reset_worker();
                }
                Err(err) => {
                    return Err(Error::StringError(format!(
                        "failed to enqueue embedding request for model '{}': {}",
                        self.model_name, err
                    )));
                }
            }
        }

        Err(Error::StringError(format!(
            "embedding worker restart attempts exhausted for model '{}'",
            self.model_name
        )))
    }
}

fn run_embedding_worker(
    worker_name: &str,
    model: &mut LocalCandleEmbeddingModel,
    receiver: Receiver<EmbeddingRequest>,
) {
    log::info!(
        "Queued embedding worker '{}' started for model '{}'",
        worker_name,
        model.get_model_name()
    );

    while let Ok(request) = receiver.recv() {
        let result =
            panic::catch_unwind(AssertUnwindSafe(|| model.batch_inference(&request.texts)));
        let response = match result {
            Ok(result) => result,
            Err(_) => {
                let _ = request.response.send(Err(Error::StringError(format!(
                    "embedding worker '{}' panicked during inference",
                    worker_name
                ))));
                break;
            }
        };

        if request.response.send(response).is_err() {
            warn!(
                "Queued embedding worker '{}' dropped a response because the requester disappeared",
                worker_name
            );
        }
    }

    log::warn!(
        "Queued embedding worker '{}' exited for model '{}'",
        worker_name,
        model.get_model_name()
    );
}

impl fmt::Debug for QueuedEmbeddingModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueuedEmbeddingModel")
            .field("model_name", &self.model_name)
            .field("queue_capacity", &self.queue_capacity)
            .finish()
    }
}

impl EmbeddingBase for QueuedEmbeddingModel {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, text: &str) -> Result<Vec<f32>> {
        let text = text.trim();
        if text.is_empty() {
            return Err(Error::ValidationError(
                "text input must not be blank".to_string(),
            ));
        }

        let mut embeddings = self.request_embeddings(vec![text.to_string()])?;
        embeddings
            .pop()
            .ok_or_else(|| Error::StringError("embedding worker returned no vectors".to_string()))
    }

    fn batch_inference(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Err(Error::ValidationError(
                "texts input must not be empty".to_string(),
            ));
        }

        self.request_embeddings(texts.to_vec())
    }
}
