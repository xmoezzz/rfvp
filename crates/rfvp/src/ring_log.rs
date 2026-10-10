//! A logger for the `no_std` builds.
//!
//! They have nowhere to print, so the engine's `log::*` messages used to be dropped, which left
//! nothing to go on when a game failed on a console. This keeps the most recent lines in memory
//! so a host can write them out when something goes wrong (see [`dump`]).

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use core::fmt::Write;

use log::{LevelFilter, Log, Metadata, Record};

/// How many lines are kept; older ones are dropped.
const CAPACITY: usize = 400;
/// Longer lines are cut, so a runaway message cannot use up the memory.
const MAX_LINE: usize = 400;

static LINES: spin::Mutex<VecDeque<String>> = spin::Mutex::new(VecDeque::new());

struct RingLogger;

static LOGGER: RingLogger = RingLogger;

impl Log for RingLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let mut line = format!("[{}] ", record.level());
        let _ = write!(line, "{}", record.args());
        if line.len() > MAX_LINE {
            let mut end = MAX_LINE;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
        }
        let mut lines = LINES.lock();
        if lines.len() >= CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    fn flush(&self) {}
}

/// Start collecting log lines (warnings and errors, and the engine's own `info` messages).
/// Returns `false` if some other logger was already installed.
pub fn install() -> bool {
    if log::set_logger(&LOGGER).is_err() {
        return false;
    }
    log::set_max_level(LevelFilter::Info);
    true
}

/// The collected lines, oldest first, one per line.
pub fn dump() -> String {
    let lines = LINES.lock();
    let mut out = String::new();
    for line in lines.iter() {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_latest_lines_and_cuts_long_ones() {
        // Another test may have installed a logger first; this one only needs the buffer.
        let _ = install();
        LINES.lock().clear();
        for i in 0..(CAPACITY + 10) {
            LOGGER.log(
                &Record::builder()
                    .level(log::Level::Error)
                    .args(format_args!("line {i}"))
                    .build(),
            );
        }
        let text = dump();
        assert_eq!(text.lines().count(), CAPACITY);
        assert!(text.contains(&format!("line {}", CAPACITY + 9)));
        assert!(!text.contains("line 0\n"), "the oldest lines are dropped");

        LINES.lock().clear();
        let long = "あ".repeat(MAX_LINE);
        LOGGER.log(
            &Record::builder()
                .level(log::Level::Error)
                .args(format_args!("{long}"))
                .build(),
        );
        let text = dump();
        assert!(text.len() <= MAX_LINE + 1, "{}", text.len());
        assert!(text.starts_with("[ERROR] "));
    }
}
