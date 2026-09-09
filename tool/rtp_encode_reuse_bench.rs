use std::hint::black_box;
use std::time::Instant;

const HEADER: usize = 12;

fn encode(payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER + payload.len());
    buf.push(0x80);
    buf.push(0);
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&160u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

fn encode_into(buf: &mut Vec<u8>, payload: &[u8]) {
    buf.clear();
    buf.reserve(HEADER + payload.len());
    buf.push(0x80);
    buf.push(0);
    buf.extend_from_slice(&1u16.to_be_bytes());
    buf.extend_from_slice(&160u32.to_be_bytes());
    buf.extend_from_slice(&1u32.to_be_bytes());
    buf.extend_from_slice(payload);
}

fn baseline(payload: &[u8], iterations: usize) -> usize {
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum += black_box(encode(black_box(payload))).len();
    }
    checksum
}

fn optimized(payload: &[u8], iterations: usize) -> usize {
    let mut buf = Vec::with_capacity(HEADER + payload.len());
    let mut checksum = 0;
    for _ in 0..iterations {
        encode_into(&mut buf, black_box(payload));
        checksum += black_box(buf.len());
    }
    checksum
}

fn measure(run: fn(&[u8], usize) -> usize, payload: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    black_box(run(payload, iterations));
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let payload = [0xFF; 160];
    let iterations = 4_000_000;
    let before = median((0..9).map(|_| measure(baseline, &payload, iterations)).collect());
    let after = median((0..9).map(|_| measure(optimized, &payload, iterations)).collect());
    println!("baseline_us={before} optimized_us={after} gain={:.1}%", (before as f64 - after as f64) * 100.0 / before as f64);
}
