pub struct ScheduledTaskManager;

impl ScheduledTaskManager {
    pub fn new() -> Self {
        Self
    }

    pub async fn start(&self) {}
}

impl Default for ScheduledTaskManager {
    fn default() -> Self {
        Self::new()
    }
}
