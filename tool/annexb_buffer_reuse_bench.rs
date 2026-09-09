use std::hint::black_box;
use std::time::Instant;

fn baseline(nal: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        let mut annexb = vec![0, 0, 0, 1];
        annexb.extend_from_slice(black_box(nal));
        checksum += black_box(annexb.len());
    }
    checksum
}

fn optimized(nal: &[u8], iterations: usize) -> usize {
    let mut annexb = Vec::with_capacity(nal.len() + 4);
    let mut checksum = 0;
    for _ in 0..iterations {
        annexb.clear();
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(black_box(nal));
        checksum += black_box(annexb.len());
    }
    checksum
}

fn measure(run: fn(&[u8], usize) -> usize, nal: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    black_box(run(nal, iterations));
    start.elapsed().as_micros()
}

fn percentile(mut values: Vec<u128>, numerator: usize, denominator: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let nal = vec![0x55; 1400];
    let iterations = 1_000_000;
    let before = (0..11)
        .map(|_| measure(baseline, &nal, iterations))
        .collect::<Vec<_>>();
    let after = (0..11)
        .map(|_| measure(optimized, &nal, iterations))
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
