use std::hint::black_box;
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

fn lines(count: usize) -> Vec<Line<'static>> {
    (0..count)
        .map(|index| {
            Line::from(vec![
                Span::styled(
                    format!("sender-{index:04}"),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(format!(
                    " message payload {index:04} with enough content to render"
                )),
            ])
        })
        .collect()
}

fn baseline(lines: &[Line<'static>], width: u16, height: u16) -> usize {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    for (row, line) in lines.iter().take(height as usize).enumerate() {
        Paragraph::new(line.clone()).render(Rect::new(0, row as u16, width, 1), &mut buffer);
    }
    black_box(buffer.content.len())
}

fn optimized(lines: &[Line<'static>], width: u16, height: u16) -> usize {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    for (row, line) in lines.iter().take(height as usize).enumerate() {
        buffer.set_line(0, row as u16, line, width);
    }
    black_box(buffer.content.len())
}

fn measure<F: Fn() -> usize>(run: F, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum += black_box(run());
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let all_lines = lines(200);
    for height in [10u16, 30, 60] {
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&all_lines, 100, height), 2_000));
            after.push(measure(|| optimized(&all_lines, 100, height), 2_000));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!(
            "visible_lines={height} baseline_us={before} optimized_us={after} gain={gain:.1}%"
        );
    }
}
