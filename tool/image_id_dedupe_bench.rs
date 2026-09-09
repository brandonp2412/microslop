use std::collections::HashSet;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn baseline(values: &[String]) -> usize {
    let mut out = Vec::new();
    for value in values {
        if !out.iter().any(|existing| existing == value) {
            out.push(value.clone());
        }
    }
    black_box(out).len()
}

fn hashed(values: &[String]) -> usize {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        if seen.insert(value.as_str()) {
            out.push(value.clone());
        }
    }
    black_box(out).len()
}

fn measure(run: fn(&[String]) -> usize, values: &[String]) -> Duration {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..20_000 {
        checksum ^= run(black_box(values));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let values = (0..128)
        .flat_map(|index| {
            let value = format!("https://example.test/image-{index}.png");
            [value.clone(), value]
        })
        .collect::<Vec<_>>();
    let mut before = (0..15).map(|_| measure(baseline, &values)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(hashed, &values)).collect::<Vec<_>>();
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
