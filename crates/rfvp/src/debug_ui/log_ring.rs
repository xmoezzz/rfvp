use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock};

/// Thread-safe ring buffer for log lines.
#[derive(Debug)]
pub struct LogRing {
    cap: usize,
    inner: Mutex<VecDeque<String>>,
}

impl LogRing {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(64),
            inner: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push_line(&self, line: String) {
        let mut q = self.inner.lock().expect("log ring poisoned");
        if q.len() >= self.cap {
            let overflow = q.len() + 1 - self.cap;
            for _ in 0..overflow {
                let _ = q.pop_front();
            }
        }
        q.push_back(line);
    }

    pub fn snapshot_tail(&self, max_lines: usize) -> Vec<String> {
        let q = self.inner.lock().expect("log ring poisoned");
        let n = max_lines.min(q.len());
        q.iter().skip(q.len().saturating_sub(n)).cloned().collect()
    }
}

static LOG_RING: OnceLock<Arc<LogRing>> = OnceLock::new();

pub fn init(cap: usize) -> Arc<LogRing> {
    LOG_RING
        .get_or_init(|| Arc::new(LogRing::new(cap)))
        .clone()
}

pub fn get() -> Option<Arc<LogRing>> {
    LOG_RING.get().cloned()
}

/// A std::io::Write sink that line-buffers into a LogRing.
///
/// fern formats log records into a stream; we split by '\n' to reconstitute lines.
pub struct RingWriter {
    ring: Arc<LogRing>,
    buf: String,
}

impl RingWriter {
    pub fn new(ring: Arc<LogRing>) -> Self {
        Self {
            ring,
            buf: String::new(),
        }
    }

    fn flush_buf_lines(&mut self, force_flush_tail: bool) {
        while let Some(pos) = self.buf.find('\n') {
            let mut line = self.buf.drain(..=pos).collect::<String>();
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            if !line.is_empty() {
                self.ring.push_line(line);
            }
        }

        if force_flush_tail {
            let tail = self.buf.trim_end_matches(['\r', '\n']).to_string();
            self.buf.clear();
            if !tail.is_empty() {
                self.ring.push_line(tail);
            }
        }
    }
}

impl Write for RingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s = String::from_utf8_lossy(buf);
        self.buf.push_str(&s);
        self.flush_buf_lines(false);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_buf_lines(true);
        Ok(())
    }
}
