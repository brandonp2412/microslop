//! Debug log pane for displaying captured tracing output in the TUI.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

use super::log_capture::LogBuffer;

/// Maximum number of accumulated lines to keep in the debug log.
///
/// This is larger than the ring buffer capacity (500) to provide more scroll
/// history for display purposes. The ring buffer acts as backpressure on writes,
/// while this limit controls how much history the user can scroll through.
const MAX_ACCUMULATED_LINES: usize = 1000;

pub struct DebugLogState {
    buffer: LogBuffer,
    lines: Vec<String>,
    /// Whether the debug log pane is visible.
    pub visible: bool,
    /// Scroll offset (0 = viewing most recent lines at bottom).
    scroll_offset: usize,
}

impl DebugLogState {
    /// Create a new debug log state with the given log buffer.
    pub fn new(buffer: LogBuffer) -> Self {
        Self {
            buffer,
            lines: Vec::new(),
            visible: false,
            scroll_offset: 0,
        }
    }

    ///
    /// Call this every event loop iteration to prevent unbounded mutex growth.
    pub fn refresh(&mut self) {
        let new_lines = self
            .buffer
            .drain()
            .into_iter()
            .filter(|line| is_error_log_line(line))
            .collect::<Vec<_>>();
        if !new_lines.is_empty() {
            self.lines.extend(new_lines);
            if self.lines.len() > MAX_ACCUMULATED_LINES {
                let excess = self.lines.len() - MAX_ACCUMULATED_LINES;
                self.lines.drain(..excess);
                self.scroll_offset = self.scroll_offset.saturating_sub(excess);
            }
        }
    }

    ///
    /// When opening, auto-scroll to the bottom (most recent logs).
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible {
            self.scroll_offset = 0;
        }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let max_offset = self.lines.len().saturating_sub(1);
        self.scroll_offset = self.scroll_offset.saturating_add(n).min(max_offset);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Return the complete log in a clipboard-friendly format.
    pub fn copy_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Copy the complete log to the platform clipboard.
    pub fn copy_to_clipboard(&self) -> std::io::Result<()> {
        let mut command = if cfg!(target_os = "macos") {
            Command::new("pbcopy")
        } else if cfg!(target_os = "windows") {
            Command::new("clip")
        } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Command::new("wl-copy")
        } else {
            let mut command = Command::new("xclip");
            command.args(["-selection", "clipboard"]);
            command
        };
        let mut child = command.stdin(Stdio::piped()).spawn()?;
        child
            .stdin
            .take()
            .expect("clipboard stdin configured")
            .write_all(self.copy_text().as_bytes())?;
        child.wait().and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other("clipboard command failed"))
            }
        })
    }

    #[cfg(test)]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

pub fn render(area: Rect, buf: &mut Buffer, state: &DebugLogState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            " Debug Log ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " c Copy logs  PgUp/PgDn Scroll ",
            Style::default().fg(Color::Gray),
        ));

    let inner = block.inner(area);
    block.render(area, buf);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let visible_lines = inner.height as usize;
    let total_lines = state.lines.len();

    let end_idx = total_lines.saturating_sub(state.scroll_offset);
    let start_idx = end_idx.saturating_sub(visible_lines);

    let lines_to_show: Vec<Line> = state.lines[start_idx..end_idx]
        .iter()
        .map(|line| colorize_log_line(line))
        .collect();

    let para = Paragraph::new(lines_to_show);
    para.render(inner, buf);
}

fn colorize_log_line(line: &str) -> Line<'_> {
    let color = if line.contains(" ERROR ") || line.contains("ERROR:") {
        Color::Red
    } else if line.contains(" WARN ") || line.contains("WARN:") {
        Color::Yellow
    } else if line.contains(" INFO ") || line.contains("INFO:") {
        Color::Green
    } else if line.contains(" DEBUG ")
        || line.contains("DEBUG:")
        || line.contains(" TRACE ")
        || line.contains("TRACE:")
    {
        Color::DarkGray
    } else {
        Color::White
    };

    Line::from(Span::styled(line, Style::default().fg(color)))
}

fn is_error_log_line(line: &str) -> bool {
    line.contains(" ERROR ") || line.contains("ERROR:") || line.starts_with("ERROR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn benchmark_log_line_borrowing() {
        use std::{hint::black_box, time::Instant};

        let line =
            "2026-09-09 INFO Teams message activity stream received a representative log line";
        let rounds = 1_000_000;
        let style = Style::default().fg(Color::Green);
        let start = Instant::now();
        for _ in 0..rounds {
            black_box(Line::from(Span::styled(line.to_owned(), style)));
        }
        let owned = start.elapsed();
        let start = Instant::now();
        for _ in 0..rounds {
            black_box(Line::from(Span::styled(line, style)));
        }
        let borrowed = start.elapsed();
        eprintln!(
            "log_line_render owned_us={} borrowed_us={} gain={:.1}%",
            owned.as_micros(),
            borrowed.as_micros(),
            (owned.as_secs_f64() - borrowed.as_secs_f64()) * 100.0 / owned.as_secs_f64()
        );
    }

    #[test]
    fn test_debug_log_refresh() {
        let buffer = LogBuffer::new();
        buffer.push("ERROR line 1".to_string());
        buffer.push("ERROR line 2".to_string());

        let mut state = DebugLogState::new(buffer.clone());
        assert_eq!(state.line_count(), 0);

        state.refresh();
        assert_eq!(state.line_count(), 2);

        buffer.push("ERROR line 3".to_string());
        state.refresh();
        assert_eq!(state.line_count(), 3);
    }

    #[test]
    fn test_debug_log_toggle() {
        let buffer = LogBuffer::new();
        let mut state = DebugLogState::new(buffer);

        assert!(!state.visible);
        state.toggle();
        assert!(state.visible);
        state.toggle();
        assert!(!state.visible);
    }

    #[test]
    fn test_debug_log_scroll() {
        let buffer = LogBuffer::new();
        for i in 0..20 {
            buffer.push(format!("ERROR line {}", i));
        }
        let mut state = DebugLogState::new(buffer);
        state.refresh();

        state.scroll_up(5);
        assert_eq!(state.scroll_offset, 5);

        state.scroll_down(3);
        assert_eq!(state.scroll_offset, 2);

        state.scroll_down(10);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn test_debug_log_scroll_clamps() {
        let buffer = LogBuffer::new();
        for i in 0..5 {
            buffer.push(format!("ERROR line {}", i));
        }
        let mut state = DebugLogState::new(buffer);
        state.refresh();

        state.scroll_up(100);
        assert_eq!(state.scroll_offset, 4);
    }

    #[test]
    fn test_copy_text_contains_all_log_lines() {
        let buffer = LogBuffer::new();
        buffer.push("ERROR first line".to_string());
        buffer.push("ERROR second line".to_string());
        let mut state = DebugLogState::new(buffer);
        state.refresh();

        assert_eq!(state.copy_text(), "ERROR first line\nERROR second line");
    }

    #[test]
    fn test_refresh_keeps_only_error_logs() {
        let buffer = LogBuffer::new();
        buffer.push("2026-08-29 INFO normal activity".to_string());
        buffer.push("2026-08-29 ERROR request failed".to_string());
        buffer.push("2026-08-29 WARN recoverable issue".to_string());
        let mut state = DebugLogState::new(buffer);

        state.refresh();

        assert_eq!(state.copy_text(), "2026-08-29 ERROR request failed");
    }

    #[test]
    fn test_render_shows_copy_action() {
        let state = DebugLogState::new(LogBuffer::new());
        let area = Rect::new(0, 0, 40, 5);
        let mut buffer = Buffer::empty(area);

        render(area, &mut buffer, &state);

        let rendered: String = (0..area.width)
            .map(|x| {
                buffer[(x, area.height - 1)]
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(rendered.contains("c Copy"));
    }
}
