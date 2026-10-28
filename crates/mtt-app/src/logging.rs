//! Minimal logger: stderr plus an append-only file next to the Node app's `app.log`.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

struct Logger {
    file: Option<Mutex<File>>,
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info || metadata.target().starts_with("mtt")
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line =
            format!("{} [{}] {}: {}\n", mtt_core::time::now_iso(), record.level(), record.target(), record.args());
        eprint!("{line}");
        if let Some(file) = &self.file
            && let Ok(mut file) = file.lock()
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file
            && let Ok(mut file) = file.lock()
        {
            let _ = file.flush();
        }
    }
}

pub fn init(path: &Path) {
    let file = OpenOptions::new().create(true).append(true).open(path).ok().map(Mutex::new);
    let logger = Box::leak(Box::new(Logger { file }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}
