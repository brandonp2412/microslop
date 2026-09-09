use std::hint::black_box;
use std::time::Instant;

const HEADER: usize = 12;

fn baseline(packet: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        let payload = black_box(packet)[HEADER..].to_vec();
        checksum += black_box(payload.len());
    }
    checksum
}

fn optimized(packet: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        let payload = &black_box(packet)[HEADER..];
        checksum += black_box(payload.len());
    }
    checksum
}

fn measure(run: fn(&[u8], usize) -> usize, packet: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    black_box(run(packet, iterations));
    start.elapsed().as_micros()
}

fn percentile(mut values: Vec<u128>, numerator: usize, denominator: usize) -> u128 {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let packet = vec![0x55; HEADER + 1400];
    let iterations = 4_000_000;
    let before = (0..11)
        .map(|_| measure(baseline, &packet, iterations))
        .collect::<Vec<_>>();
    let after = (0..11)
        .map(|_| measure(optimized, &packet, iterations))
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
