use std::hint::black_box;
use std::time::Instant;

const SAMPLES: usize = 160;
const SILENCE: [u8; SAMPLES] = [0xFF; SAMPLES];

fn encode(payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12 + payload.len());
    packet.extend_from_slice(&[0u8; 12]);
    packet.extend_from_slice(payload);
    packet
}

fn baseline() -> usize {
    let samples = vec![0i16; SAMPLES];
    let mut payload = Vec::with_capacity(SAMPLES);
    for sample in samples {
        payload.push(if sample == 0 { 0xFF } else { 0 });
    }
    encode(&payload).len()
}

fn optimized() -> usize {
    encode(&SILENCE).len()
}

fn measure(run: fn() -> usize, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(run()));
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let iterations = 2_000_000;
    let before = median((0..9).map(|_| measure(baseline, iterations)).collect());
    let after = median((0..9).map(|_| measure(optimized, iterations)).collect());
    println!("baseline_us={before} optimized_us={after} gain={:.1}%", (before as f64 - after as f64) * 100.0 / before as f64);
}
