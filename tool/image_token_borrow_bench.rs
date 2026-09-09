use std::hint::black_box;
use std::time::{Duration, Instant};

fn cloned(token: &String, count: usize) -> usize {
    (0..count)
        .map(|_| token.clone())
        .map(|token| black_box(token).len())
        .sum()
}

fn borrowed(token: &String, count: usize) -> usize {
    (0..count).map(|_| black_box(token).len()).sum()
}

fn measure(run: fn(&String, usize) -> usize, token: &String, count: usize) -> Duration {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..2_000_000 {
        checksum ^= black_box(run(black_box(token), black_box(count)));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let token = "header.payload.signature".repeat(96);
    let mut before = (0..15).map(|_| measure(cloned, &token, 4)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(borrowed, &token, 4)).collect::<Vec<_>>();
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
