// NOTE(release-strip): This module produces a verbose debug.log next to the exe.
// Before releasing, gate every call to `dlog!` behind `#[cfg(debug_assertions)]` or
// drop the file write path entirely. The log captures yt-dlp args, full stdout/stderr,
// spawn errors, and event emissions — useful while iterating, but not for end users.

use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: OnceLock<Mutex<Option<File>>> = OnceLock::new();

fn log_path() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    dir.join("debug.log")
}

pub fn init() {
    // Debug log is debug-build only. Release leaves FILE unset so write_line is a no-op
    // (and the dlog! macro itself is compiled out in release).
    #[cfg(debug_assertions)]
    {
        let path = log_path();
        let f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .ok();
        let cell = FILE.get_or_init(|| Mutex::new(None));
        *cell.lock() = f;
        write_line("=== yt-portable session start ===");
    }
}

fn iso_ts() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let ms = now.subsec_millis();
    // Local-time formatting would need a tz crate; epoch ms is enough for ordering.
    format!("{}.{:03}", secs, ms)
}

pub fn write_line(line: &str) {
    if let Some(cell) = FILE.get() {
        if let Some(f) = cell.lock().as_mut() {
            let _ = writeln!(f, "[{}] {}", iso_ts(), line);
            let _ = f.flush();
        }
    }
}

#[macro_export]
macro_rules! dlog {
    ($($arg:tt)*) => {{
        #[cfg(debug_assertions)]
        $crate::debug::write_line(&format!($($arg)*));
    }};
}
