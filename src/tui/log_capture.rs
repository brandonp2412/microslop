//! Log capture for TUI mode
//!
//! Provides a ring buffer that implements `MakeWriter` so tracing-subscriber
//! can write log lines here instead of stderr. This prevents tracing output
//! from corrupting the ratatui alternate screen.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;

///
/// This is the backpressure limit on the write path. The display side
/// (`DebugLogState`) accumulates up to a larger limit (1000) for scroll history.
const RING_BUFFER_CAPACITY: usize = 500;

///
/// Clone is derived to satisfy the `MakeWriter` trait which requires creating
#[derive(Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<String>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(RING_BUFFER_CAPACITY))),
        }
    }

    ///
    /// If the buffer is at capacity, the oldest line is removed.
    /// If the mutex is poisoned (another thread panicked), we recover the
    pub fn push(&self, line: String) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if guard.len() >= RING_BUFFER_CAPACITY {
            guard.pop_front();
        }
        guard.push_back(line);
    }

    ///
    /// Returns the lines in order (oldest first) and clears the buffer.
    /// This is designed for single-consumer use; if multiple consumers call
    pub fn drain(&self) -> Vec<String> {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        guard.drain(..).collect()
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// A writer that buffers bytes and flushes complete lines to a LogBuffer.
pub struct BufferWriter {
    buffer: LogBuffer,
    pending: Vec<u8>,
}

impl BufferWriter {
    fn new(buffer: LogBuffer) -> Self {
        Self {
            buffer,
            pending: Vec::new(),
        }
    }

    fn flush_lines(&mut self) {
        let Some(last_newline) = self.pending.iter().rposition(|&b| b == b'\n') else {
            return;
        };
        for line in self.pending[..last_newline].split(|&b| b == b'\n') {
            self.buffer.push(String::from_utf8_lossy(line).into_owned());
        }
        self.pending.drain(..=last_newline);
    }
}

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(buf);
        self.flush_lines();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let s = String::from_utf8_lossy(&self.pending).into_owned();
            self.buffer.push(s);
            self.pending.clear();
        }
        Ok(())
    }
}

impl Drop for BufferWriter {
    fn drop(&mut self) {
        let _ = Write::flush(self);
    }
}

impl<'a> MakeWriter<'a> for LogBuffer {
    type Writer = BufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        BufferWriter::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_log_buffer_basic() {
        let buf = LogBuffer::new();
        buf.push("line 1".to_string());
        buf.push("line 2".to_string());

        let lines = buf.drain();
        assert_eq!(lines, vec!["line 1", "line 2"]);

        assert!(buf.drain().is_empty());
    }

    #[test]
    fn test_log_buffer_capacity() {
        let buf = LogBuffer::new();
        for i in 0..600 {
            buf.push(format!("line {}", i));
        }

        let lines = buf.drain();
        assert_eq!(lines.len(), 500);
        assert_eq!(lines[0], "line 100");
        assert_eq!(lines[499], "line 599");
    }

    #[test]
    fn test_buffer_writer_lines() {
        let buf = LogBuffer::new();
        let mut writer = BufferWriter::new(buf.clone());

        write!(writer, "hello\nworld\n").unwrap();

        let lines = buf.drain();
        assert_eq!(lines, vec!["hello", "world"]);
    }

    #[test]
    fn test_buffer_writer_partial() {
        let buf = LogBuffer::new();
        {
            let mut writer = BufferWriter::new(buf.clone());
            write!(writer, "one\n\npartial").unwrap();
            assert_eq!(buf.drain(), vec!["one", ""]);
        }

        let lines = buf.drain();
        assert_eq!(lines, vec!["partial"]);
    }
}
