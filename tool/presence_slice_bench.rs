use std::hint::black_box;
use std::time::{Duration, Instant};

fn cloned(user_ids: &[String]) -> usize {
    user_ids
        .iter()
        .take(650)
        .cloned()
        .collect::<Vec<_>>()
        .iter()
        .map(String::len)
        .sum()
}

fn borrowed(user_ids: &[String]) -> usize {
    user_ids[..user_ids.len().min(650)].iter().map(String::len).sum()
}

fn measure(run: fn(&[String]) -> usize, user_ids: &[String], iterations: usize) -> Duration {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum ^= black_box(run(black_box(user_ids)));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let user_ids = (0..650)
        .map(|index| format!("{index:08x}-{index:04x}-{index:04x}-{index:04x}-{index:012x}"))
        .collect::<Vec<_>>();
    let mut before = (0..15)
        .map(|_| measure(cloned, &user_ids, 20_000))
        .collect::<Vec<_>>();
    let mut after = (0..15)
        .map(|_| measure(borrowed, &user_ids, 20_000))
        .collect::<Vec<_>>();
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
