//! Launcher logger: redacted, to `logs/launcher.log` and an in-memory ring
//! buffer shown on the Logs page.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};

use kestrel_core::redact::redact;

pub const MAX_LINES: usize = 4000;

#[derive(Clone, Default)]
pub struct LogBuffer(Arc<Mutex<VecDeque<String>>>);

impl LogBuffer {
    pub fn push(&self, line: String) {
        if let Ok(mut b) = self.0.lock() {
            if b.len() >= MAX_LINES {
                b.pop_front();
            }
            b.push_back(line);
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|b| b.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut b) = self.0.lock() {
            b.clear();
        }
    }
}

struct Logger {
    file: Mutex<Option<File>>,
    buffer: LogBuffer,
}

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info || m.target().starts_with("kestrel")
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = redact(&format!(
            "[{}] [{}] {}",
            kestrel_core::auth::now_secs(),
            record.level(),
            record.args()
        ));
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = writeln!(f, "{line}");
        }
        self.buffer.push(line);
    }

    fn flush(&self) {
        if let Ok(mut f) = self.file.lock()
            && let Some(f) = f.as_mut()
        {
            let _ = f.flush();
        }
    }
}

/// Install the global logger. Safe to call once; later calls are ignored.
pub fn init(log_dir: &Path, buffer: LogBuffer) {
    let _ = std::fs::create_dir_all(log_dir);
    let path = log_dir.join("launcher.log");
    // Keep one previous log for crash diagnostics.
    let _ = std::fs::rename(&path, log_dir.join("launcher.prev.log"));
    let file = File::create(&path).ok();
    let logger = Logger {
        file: Mutex::new(file),
        buffer,
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}
