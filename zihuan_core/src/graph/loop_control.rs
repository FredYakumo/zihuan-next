use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::data_value::DataValue;

/// Shared loop control state, passed from LoopNode to LoopBreakNode via LoopControlRef ports.
pub struct LoopControl {
    break_flag: AtomicBool,
    /// Carries the "current iteration data" so downstream nodes can write back
    /// updated state without creating a data-edge cycle in the graph.
    current_state: Mutex<DataValue>,
}

impl fmt::Debug for LoopControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoopControl")
            .field("break_flag", &self.break_flag.load(Ordering::Relaxed))
            .field("current_state", &"<Mutex<DataValue>>")
            .finish()
    }
}

impl LoopControl {
    pub fn new() -> Self {
        Self {
            break_flag: AtomicBool::new(false),
            current_state: Mutex::new(DataValue::Boolean(false)),
        }
    }
    pub fn request_break(&self) {
        self.break_flag.store(true, Ordering::SeqCst);
    }
    pub fn should_break(&self) -> bool {
        self.break_flag.load(Ordering::SeqCst)
    }
    pub fn reset(&self) {
        self.break_flag.store(false, Ordering::SeqCst);
    }
    /// Called by LoopNode on_start to set the initial state.
    pub fn init_state(&self, v: DataValue) {
        *self.current_state.lock().unwrap() = v;
    }
    /// Called by LoopNode on_update to read the current state.
    pub fn get_state(&self) -> DataValue {
        self.current_state.lock().unwrap().clone()
    }
    /// Called by LoopStateUpdateNode to write the updated state for the next iteration.
    pub fn update_state(&self, v: DataValue) {
        *self.current_state.lock().unwrap() = v;
    }
}
