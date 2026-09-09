use std::borrow::Cow;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn is_inline_emoji_tag(tag: &str) -> bool {
    let name = tag
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_ascii_lowercase();
    matches!(name.as_str(), "emoji" | "customemoji") || tag.contains("schema.skype.com/Emoji")
}

fn baseline(html: &str) -> Cow<'_, str> {
    let mut result = String::with_capacity(html.len());
    let mut remainder = html;
    while let Some(start) = remainder.find('<') {
        result.push_str(&remainder[..start]);
        remainder = &remainder[start..];
        let Some(end) = remainder.find('>') else {
            result.push_str(remainder);
            return Cow::Owned(result);
        };
        if !is_inline_emoji_tag(&remainder[1..end]) {
            result.push_str(&remainder[..=end]);
        }
        remainder = &remainder[end + 1..];
    }
    result.push_str(remainder);
    Cow::Owned(result)
}

fn borrowed(html: &str) -> Cow<'_, str> {
    let mut remainder = html;
    let has_inline = loop {
        let Some(start) = remainder.find('<') else {
            break false;
        };
        remainder = &remainder[start..];
        let Some(end) = remainder.find('>') else {
            break false;
        };
        if is_inline_emoji_tag(&remainder[1..end]) {
            break true;
        }
        remainder = &remainder[end + 1..];
    };
    if !has_inline {
        return Cow::Borrowed(html);
    }
    baseline(html)
}

fn measure(run: fn(&str) -> Cow<'_, str>, html: &str) -> Duration {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..100_000 {
        checksum ^= black_box(run(black_box(html))).len();
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let html = (0..20)
        .map(|index| format!("<p>Normal Teams message block {index} with no inline emoji.</p>"))
        .collect::<String>();
    let mut before = (0..15).map(|_| measure(baseline, &html)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(borrowed, &html)).collect::<Vec<_>>();
    let before_p50 = percentile(&mut before, 1, 2);
    let after_p50 = percentile(&mut after, 1, 2);
    let before_p95 = percentile(&mut before, 19, 20);
    let after_p95 = percentile(&mut after, 19, 20);
    println!(
        "p50_us={}->{} gain={:.1}% p95_us={}->{} gain={:.1}%",
        before_p50.as_micros(),
        after_p50.as_micros(),
        (before_p50.as_secs_f64() - after_p50.as_secs_f64()) * 100.0 / before_p50.as_secs_f64(),
        before_p95.as_micros(),
        after_p95.as_micros(),
        (before_p95.as_secs_f64() - after_p95.as_secs_f64()) * 100.0 / before_p95.as_secs_f64(),
    );
}
