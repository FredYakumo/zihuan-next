use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

use crate::orchestrator::SetupProgressEvent;

pub struct AppState {
    pub setup_tasks: Mutex<HashMap<String, broadcast::Sender<SetupProgressEvent>>>,
    pub token: String,
    pub history: Arc<Mutex<HashMap<String, Vec<SetupProgressEvent>>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            setup_tasks: Mutex::new(HashMap::new()),
            token: uuid::Uuid::new_v4().to_string(),
            history: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register_task(&self, id: String, sender: broadcast::Sender<SetupProgressEvent>) {
        let mut receiver = sender.subscribe();
        self.history.lock().unwrap().insert(id.clone(), Vec::new());
        self.setup_tasks.lock().unwrap().insert(id.clone(), sender);
        let history = self.history.clone();
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => history.lock().unwrap().entry(id.clone()).or_default().push(event),
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
    }
}
