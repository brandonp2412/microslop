use std::hint::black_box;
use std::time::Instant;

const SAMPLES_PER_PACKET: usize = 160;
const SILENCE: [u8; SAMPLES_PER_PACKET] = [0xFF; SAMPLES_PER_PACKET];

fn encode(payload: &[u8]) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12 + payload.len());
    packet.extend_from_slice(&[0x80, 0x00]);
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&160u32.to_be_bytes());
    packet.extend_from_slice(&1u32.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn baseline() -> usize {
    let silence = vec![0xFF; SAMPLES_PER_PACKET];
    encode(&silence).len()
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
