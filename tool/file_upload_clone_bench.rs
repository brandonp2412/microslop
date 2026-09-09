use std::hint::black_box;
use std::time::Instant;

fn baseline(data: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        let body = black_box(data).to_vec();
        checksum += black_box(body.len());
    }
    checksum
}

fn optimized(data: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        let body = black_box(data);
        checksum += black_box(body.len());
    }
    checksum
}

fn measure(run: fn(&[u8], usize) -> usize, data: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    black_box(run(data, iterations));
    start.elapsed().as_micros()
}

fn percentile(mut values: Vec<u128>, numerator: usize, denominator: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let data = vec![0x55; 8 * 1024 * 1024];
    let iterations = 200;
    let before = (0..11)
        .map(|_| measure(baseline, &data, iterations))
        .collect::<Vec<_>>();
    let after = (0..11)
        .map(|_| measure(optimized, &data, iterations))
        .collect::<Vec<_>>();
    let before_p50 = percentile(before.clone(), 1, 2);
    let after_p50 = percentile(after.clone(), 1, 2);
    let before_p95 = percentile(before, 19, 20);
    let after_p95 = percentile(after, 19, 20);
    println!(
        "p50_us={before_p50}->{after_p50} gain={:.1}% p95_us={before_p95}->{after_p95} gain={:.1}%",
        (before_p50 as f64 - after_p50 as f64) * 100.0 / before_p50 as f64,
        (before_p95 as f64 - after_p95 as f64) * 100.0 / before_p95 as f64,
    );
}
