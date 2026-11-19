//! FIFO logical turn reservations. No mutex guard is held while a turn runs or waits.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

#[derive(Default)]
struct State {
    next: u64,
    waiting: VecDeque<(u64, bool)>,
    readers: usize,
    writer: bool,
}
#[derive(Default)]
pub struct TurnQueue {
    state: Mutex<State>,
    changed: Notify,
}
pub struct TurnReservation {
    queue: Arc<TurnQueue>,
    id: u64,
    shared: bool,
    active: bool,
}
impl TurnQueue {
    pub async fn reserve(self: Arc<Self>, shared: bool) -> TurnReservation {
        let id = {
            let mut s = self.state.lock().unwrap();
            let id = s.next;
            s.next += 1;
            s.waiting.push_back((id, shared));
            id
        };
        let mut reservation = TurnReservation {
            queue: self.clone(),
            id,
            shared,
            active: false,
        };
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut s = self.state.lock().unwrap();
                if s.waiting.front().is_some_and(|(next, _)| *next == id)
                    && !s.writer
                    && (shared || s.readers == 0)
                {
                    s.waiting.pop_front();
                    if shared {
                        s.readers += 1;
                    } else {
                        s.writer = true;
                    }
                    reservation.active = true;
                    self.changed.notify_waiters();
                    return reservation;
                }
            }
            changed.await;
        }
    }
}
impl Drop for TurnReservation {
    fn drop(&mut self) {
        let mut s = self.queue.state.lock().unwrap();
        if self.active {
            if self.shared {
                s.readers -= 1;
            } else {
                s.writer = false;
            }
        } else {
            s.waiting.retain(|(id, _)| *id != self.id);
        }
        drop(s);
        self.queue.changed.notify_waiters();
    }
}
