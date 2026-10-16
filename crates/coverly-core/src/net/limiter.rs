//! One limiter shared by every request of a job (spec §16.5): at most
//! `concurrency` requests in flight (1–6) and at least `min_interval`
//! between two request starts.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

pub struct Limiter {
    state: Mutex<State>,
    freed: Condvar,
    capacity: usize,
    min_interval: Duration,
}

struct State {
    in_flight: usize,
    next_start: Instant,
}

/// A slot in the limiter; dropping it frees the slot.
#[must_use = "the slot is released as soon as the permit is dropped"]
pub struct Permit<'a> {
    limiter: &'a Limiter,
}

impl Limiter {
    pub fn new(concurrency: usize, min_interval: Duration) -> Arc<Limiter> {
        Arc::new(Limiter {
            state: Mutex::new(State {
                in_flight: 0,
                next_start: Instant::now(),
            }),
            freed: Condvar::new(),
            capacity: concurrency.clamp(1, 6),
            min_interval,
        })
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn acquire(&self) -> Permit<'_> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        while state.in_flight >= self.capacity {
            state = self.freed.wait(state).unwrap_or_else(|p| p.into_inner());
        }
        state.in_flight += 1;
        let now = Instant::now();
        let start = state.next_start.max(now);
        state.next_start = start + self.min_interval;
        drop(state);
        if start > now {
            std::thread::sleep(start - now);
        }
        Permit { limiter: self }
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.limiter.state.lock().unwrap_or_else(|p| p.into_inner());
        state.in_flight -= 1;
        self.limiter.freed.notify_one();
    }
}
