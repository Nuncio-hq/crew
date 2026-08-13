//! Monotonic millisecond clock so tests can advance idle timers.
//! Production uses wall time; tests freeze and `advance`.

#![allow(dead_code)]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    now_ms: u64,
    wall: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self {
            now_ms: unix_ms(),
            wall: true,
        }
    }
}

impl Clock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms,
            wall: false,
        }
    }

    pub fn now(&self) -> u64 {
        if self.wall {
            unix_ms()
        } else {
            self.now_ms
        }
    }

    pub fn set(&mut self, now_ms: u64) {
        self.wall = false;
        self.now_ms = now_ms;
    }

    pub fn advance(&mut self, delta_ms: u64) {
        self.wall = false;
        self.now_ms = self.now_ms.saturating_add(delta_ms);
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
