/**************************************************************
SPDX License Identifier: GPL-3

Authors: Arnav Waghdhare <arnavwaghdhare@gmail.com>
***************************************************************/

pub mod app;
pub mod auth;
pub mod gmail;
pub mod models;

pub mod logging {
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        path::Path,
        sync::{Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    static LOGGER: OnceLock<Mutex<File>> = OnceLock::new();

    /// Initializes file logging. Safe to call multiple times; only first call initializes.
    ///
    /// This creates the parent directory (if needed) and truncates the log file
    /// so each app launch starts with a fresh log.
    pub fn init(log_path: &str) -> std::io::Result<()> {
        let path = Path::new(log_path);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        let _ = LOGGER.set(Mutex::new(file));
        info(&format!("Logger initialized at {}", log_path));
        Ok(())
    }

    pub fn info(message: &str) {
        write_line("INFO", message);
    }

    pub fn warn(message: &str) {
        write_line("WARN", message);
    }

    pub fn error(message: &str) {
        write_line("ERROR", message);
    }

    fn write_line(level: &str, message: &str) {
        let ts = unix_ts();
        let line = format!("[{}] [{}] {}\n", ts, level, message);

        if let Some(lock) = LOGGER.get() {
            if let Ok(mut file) = lock.lock() {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    fn unix_ts() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}
