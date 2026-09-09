use std::hint::black_box;
use std::time::{Duration, Instant};

fn allocated(tag: &str) -> bool {
    let tag_name = tag
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_ascii_lowercase();
    matches!(tag_name.as_str(), "emoji" | "customemoji")
}

fn borrowed(tag: &str) -> bool {
    let tag_name = tag
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('/')
        .trim_end_matches('/');
    tag_name.eq_ignore_ascii_case("emoji") || tag_name.eq_ignore_ascii_case("customemoji")
}

fn measure(run: fn(&str) -> bool, tags: &[String]) -> Duration {
    let start = Instant::now();
    let mut checksum = false;
    for _ in 0..100_000 {
        for tag in tags {
            checksum ^= black_box(run(black_box(tag)));
        }
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let tags = (0..24)
        .map(|index| match index % 8 {
            0 => String::from("EMOJI itemtype='x'"),
            1 => String::from("CustomEmoji src='x'"),
            _ => format!("span class='message-{index}'"),
        })
        .collect::<Vec<_>>();
    let mut before = (0..15).map(|_| measure(allocated, &tags)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(borrowed, &tags)).collect::<Vec<_>>();
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
