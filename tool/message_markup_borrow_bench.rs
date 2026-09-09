use std::borrow::Cow;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn cloned(html: &str, attachment: Option<&str>) -> usize {
    let mut markup = html.to_owned();
    if let Some(attachment) = attachment {
        markup.push_str(attachment);
    }
    black_box(markup.as_bytes()).len()
}

fn borrowed(html: &str, attachment: Option<&str>) -> usize {
    let mut markup = Cow::Borrowed(html);
    if let Some(attachment) = attachment {
        markup.to_mut().push_str(attachment);
    }
    black_box(markup.as_bytes()).len()
}

fn measure(run: fn(&str, Option<&str>) -> usize, html: &str, attachment: Option<&str>) -> Duration {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..2_000_000 {
        checksum ^= black_box(run(black_box(html), black_box(attachment)));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn report(label: &str, html: &str, attachment: Option<&str>) {
    let mut before = (0..15)
        .map(|_| measure(cloned, html, attachment))
        .collect::<Vec<_>>();
    let mut after = (0..15)
        .map(|_| measure(borrowed, html, attachment))
        .collect::<Vec<_>>();
    let before_p50 = percentile(&mut before, 1, 2);
    let after_p50 = percentile(&mut after, 1, 2);
    let before_p95 = percentile(&mut before, 19, 20);
    let after_p95 = percentile(&mut after, 19, 20);
    println!(
        "{label} p50_us={}->{} gain={:.1}% p95_us={}->{} gain={:.1}%",
        before_p50.as_micros(),
        after_p50.as_micros(),
        (before_p50.as_secs_f64() - after_p50.as_secs_f64()) * 100.0 / before_p50.as_secs_f64(),
        before_p95.as_micros(),
        after_p95.as_micros(),
        (before_p95.as_secs_f64() - after_p95.as_secs_f64()) * 100.0 / before_p95.as_secs_f64(),
    );
}

fn main() {
    let html = "<p>Teams message with ordinary text and a link https://example.com/details</p>".repeat(24);
    report("no_attachment", &html, None);
    report("with_attachment", &html, Some("<img src=\"https://example.com/image.png\">"));
}
