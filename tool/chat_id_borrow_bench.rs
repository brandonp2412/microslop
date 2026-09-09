use std::hint::black_box;
use std::time::{Duration, Instant};

fn cloned(ids: &[String]) -> usize {
    ids.iter()
        .filter(|id| id.contains("thread") || id.contains("meeting"))
        .map(|id| id.to_owned())
        .map(|id| black_box(id).len())
        .sum()
}

fn borrowed(ids: &[String]) -> usize {
    ids.iter()
        .map(String::as_str)
        .filter(|id| id.contains("thread") || id.contains("meeting"))
        .map(|id| black_box(id).len())
        .sum()
}

fn measure(run: fn(&[String]) -> usize, ids: &[String]) -> Duration {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..50_000 {
        checksum ^= black_box(run(black_box(ids)));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let ids = (0..256)
        .map(|index| format!("19:meeting_{index:032x}@thread.v2"))
        .collect::<Vec<_>>();
    let mut before = (0..15).map(|_| measure(cloned, &ids)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(borrowed, &ids)).collect::<Vec<_>>();
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
