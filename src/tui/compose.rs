//! Compose box: text input with formatting toolbar and send button.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
    Frame,
};

#[derive(Default)]
pub struct ComposeState {
    pub input: String,
    pub cursor_pos: usize,
}

impl ComposeState {
    pub fn insert_char(&mut self, c: char) {
        let byte_pos = self.char_to_byte(self.cursor_pos);
        self.input.insert(byte_pos, c);
        self.cursor_pos += 1;
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            let byte_pos = self.char_to_byte(self.cursor_pos);
            let prev_byte_pos = self.char_to_byte(self.cursor_pos - 1);
            self.input.drain(prev_byte_pos..byte_pos);
            self.cursor_pos -= 1;
        }
    }

    pub fn delete(&mut self) {
        let char_count = self.input.chars().count();
        if self.cursor_pos < char_count {
            let byte_pos = self.char_to_byte(self.cursor_pos);
            let next_byte_pos = self.char_to_byte(self.cursor_pos + 1);
            self.input.drain(byte_pos..next_byte_pos);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let char_count = self.input.chars().count();
        if self.cursor_pos < char_count {
            self.cursor_pos += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_pos = self.input.chars().count();
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor_pos = 0;
    }

    /// "Send" the message: return the current text and clear the box.
    /// Returns None if the input is empty or whitespace-only.
    pub fn send(&mut self) -> Option<String> {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return None;
        }
        self.input.clear();
        self.cursor_pos = 0;
        Some(text)
    }

    fn char_to_byte(&self, char_pos: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }
}

/// Height of the compose box: 1 border + 1 toolbar + 1 input + 1 border = 4 lines.
pub const COMPOSE_HEIGHT: u16 = 4;

///
///   - Toolbar line: B  I  U  ~  link  clip  :)  ... >
///
/// Uses `Frame` directly so we can both write to the buffer and set cursor.
pub fn render(
    area: Rect,
    frame: &mut Frame,
    state: &ComposeState,
    channel_name: &str,
    focused: bool,
) {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let border_type = if focused {
        BorderType::Double
    } else {
        BorderType::Plain
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style);

    let inner = block.inner(area);

    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let toolbar_area = Rect::new(inner.x, inner.y, inner.width, 1);
    render_toolbar(toolbar_area, frame.buffer_mut(), focused);

    if inner.height >= 2 {
        let input_area = Rect::new(inner.x, inner.y + 1, inner.width, 1);

        let cursor = compute_cursor_position(input_area, state, focused);

        render_input(input_area, frame.buffer_mut(), state, channel_name);

        if let Some((cx, cy)) = cursor {
            frame.set_cursor_position((cx, cy));
        }
    }
}

/// Compute the cursor position if the compose box is focused.
/// Returns Some((x, y)) or None if not focused.
fn compute_cursor_position(
    input_area: Rect,
    state: &ComposeState,
    focused: bool,
) -> Option<(u16, u16)> {
    if !focused {
        return None;
    }

    if state.input.is_empty() {
        Some((input_area.x + 1, input_area.y))
    } else {
        let w = input_area.width as usize;
        let display = compose_display_text(&state.input, state.cursor_pos, w);
        let cursor_x = input_area.x + 1 + display.cursor_offset as u16;
        Some((cursor_x, input_area.y))
    }
}

fn render_toolbar(area: Rect, buf: &mut Buffer, focused: bool) {
    let w = area.width as usize;

    let left_items = " B  I  U  ~  @  +  :)";
    let right_items = "[cam]  [mic]  >";

    let left_style = if focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let right_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let left_w = unicode_width::UnicodeWidthStr::width(left_items);
    let right_w = unicode_width::UnicodeWidthStr::width(right_items) + 1;
    let padding = w.saturating_sub(left_w + right_w);

    let line = Line::from(vec![
        Span::styled(left_items, left_style),
        Span::raw(" ".repeat(padding)),
        Span::styled(right_items, right_style),
        Span::raw(" "),
    ]);

    Paragraph::new(line).render(area, buf);
}

fn render_input(area: Rect, buf: &mut Buffer, state: &ComposeState, channel_name: &str) {
    let w = area.width as usize;

    if state.input.is_empty() {
        let placeholder = format!(" Type a message to {}...", channel_name);
        let style = Style::default().fg(Color::DarkGray);
        let truncated: String = placeholder.chars().take(w).collect();
        let line = Line::from(Span::styled(truncated, style));
        Paragraph::new(line).render(area, buf);
    } else {
        let display = compose_display_text(&state.input, state.cursor_pos, w);
        let line = Line::from(Span::styled(
            format!(" {}", display.visible),
            Style::default().fg(Color::White),
        ));
        Paragraph::new(line).render(area, buf);
    }
}

/// Information about what text to display and where the cursor is.
struct DisplayText {
    /// The visible portion of text to render.
    visible: String,
    /// The cursor offset within the visible text (in columns).
    cursor_offset: usize,
}

/// Compute the visible text and cursor offset for display.
///
/// For multi-line input, newlines are shown as " | " separators on the single
/// display line. Horizontal scrolling keeps the cursor visible.
fn compose_display_text(input: &str, cursor_pos: usize, width: usize) -> DisplayText {
    let flat: String = input.replace('\n', " | ");

    let mut flat_cursor: usize = 0;
    for (char_idx, ch) in input.chars().enumerate() {
        if char_idx == cursor_pos {
            break;
        }
        if ch == '\n' {
            flat_cursor += 3;
        } else {
            flat_cursor += 1;
        }
    }

    let avail = width.saturating_sub(1);

    if avail == 0 {
        return DisplayText {
            visible: String::new(),
            cursor_offset: 0,
        };
    }

    let flat_chars: Vec<char> = flat.chars().collect();
    let flat_len = flat_chars.len();

    if flat_len <= avail {
        DisplayText {
            visible: flat,
            cursor_offset: flat_cursor,
        }
    } else {
        let scroll_start = if flat_cursor < avail {
            0
        } else {
            flat_cursor - avail + 1
        };
        let end = (scroll_start + avail).min(flat_len);
        let visible: String = flat_chars[scroll_start..end].iter().collect();
        let cursor_offset = flat_cursor - scroll_start;

        DisplayText {
            visible,
            cursor_offset,
        }
    }
}
