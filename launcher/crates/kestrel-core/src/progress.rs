//! Thread-safe progress reporting shared between worker threads and the UI.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct Progress {
    stage: Mutex<String>,
    done: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
}

pub type SharedProgress = Arc<Progress>;

impl Progress {
    pub fn new() -> SharedProgress {
        Arc::new(Progress::default())
    }

    /// Start a new stage with `total` units of work (0 = indeterminate).
    pub fn stage(&self, name: impl Into<String>, total: u64) {
        let name = name.into();
        log::info!("stage: {name}");
        if let Ok(mut s) = self.stage.lock() {
            *s = name;
        }
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.done.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> (String, u64, u64) {
        let stage = self.stage.lock().map(|s| s.clone()).unwrap_or_default();
        (
            stage,
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn check_cancel(&self) -> crate::Result<()> {
        if self.is_cancelled() {
            Err(crate::Error::Cancelled)
        } else {
            Ok(())
        }
    }
}
