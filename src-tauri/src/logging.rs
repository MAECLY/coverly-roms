//! A small file logger: `<data dir>/logs/coverly.log`, rotated once at
//! 1 MiB, level from the Advanced settings (applied live). Messages are
//! one line each (file names are hostile: newlines are escaped). Never
//! logs credential values (none reach this layer).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use coverly_core::settings::LogLevel;
use log::{LevelFilter, Log, Metadata, Record};

pub const LOG_FILE: &str = "coverly.log";
pub const MAX_LOG_BYTES: u64 = 1024 * 1024;

pub fn level_filter(l: LogLevel) -> LevelFilter {
    match l {
        LogLevel::Error => LevelFilter::Error,
        LogLevel::Warn => LevelFilter::Warn,
        LogLevel::Info => LevelFilter::Info,
        LogLevel::Debug => LevelFilter::Debug,
    }
}

pub struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileLogger {
    pub fn new(dir: &Path) -> FileLogger {
        FileLogger {
            path: dir.join(LOG_FILE),
            file: Mutex::new(None),
        }
    }

    fn write_line(&self, line: &str) {
        let Ok(mut slot) = self.file.lock() else {
            return;
        };
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
            *slot = None;
            let _ = fs::rename(&self.path, self.path.with_extension("log.1"));
        }
        if slot.is_none() {
            if let Some(dir) = self.path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            *slot = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok();
        }
        if let Some(f) = slot.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let msg = record
            .args()
            .to_string()
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        let line = format!(
            "{} {:<5} {}: {msg}\n",
            coverly_core::cache::now_unix(),
            record.level(),
            record.target()
        );
        self.write_line(&line);
    }

    fn flush(&self) {
        if let Ok(mut slot) = self.file.lock() {
            if let Some(f) = slot.as_mut() {
                let _ = f.flush();
            }
        }
    }
}

/// Installs the logger once (later calls only set the level).
pub fn init(dir: &Path, level: LogLevel) -> bool {
    let installed = log::set_boxed_logger(Box::new(FileLogger::new(dir))).is_ok();
    set_level(level);
    installed
}

pub fn set_level(level: LogLevel) {
    log::set_max_level(level_filter(level));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_map_one_to_one() {
        assert_eq!(level_filter(LogLevel::Error), LevelFilter::Error);
        assert_eq!(level_filter(LogLevel::Debug), LevelFilter::Debug);
    }

    #[test]
    fn writes_one_line_per_record_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(dir.path());
        log::set_max_level(LevelFilter::Info);
        logger.log(
            &Record::builder()
                .args(format_args!("opened\n<evil>"))
                .level(log::Level::Info)
                .target("t")
                .build(),
        );
        let text = fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("opened\\n<evil>"));
        fs::write(
            dir.path().join(LOG_FILE),
            vec![b'x'; MAX_LOG_BYTES as usize + 1],
        )
        .unwrap();
        logger.log(
            &Record::builder()
                .args(format_args!("after"))
                .level(log::Level::Warn)
                .target("t")
                .build(),
        );
        assert!(dir.path().join("coverly.log.1").exists());
        assert!(fs::read_to_string(dir.path().join(LOG_FILE))
            .unwrap()
            .contains("after"));
    }
}
