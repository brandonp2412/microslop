use std::hint::black_box;
use std::time::Instant;

const RTP_HEADER: usize = 12;

fn baseline(chunk: &[u8], fu_indicator: u8, fu_header: u8) -> Vec<u8> {
    let mut payload = Vec::with_capacity(2 + chunk.len());
    payload.push(fu_indicator);
    payload.push(fu_header);
    payload.extend_from_slice(chunk);

    let mut packet = Vec::with_capacity(RTP_HEADER + payload.len());
    packet.extend_from_slice(&[0x80, 0x6b, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3]);
    packet.extend_from_slice(&payload);
    packet
}

fn optimized(chunk: &[u8], fu_indicator: u8, fu_header: u8) -> Vec<u8> {
    let mut packet = Vec::with_capacity(RTP_HEADER + 2 + chunk.len());
    packet.extend_from_slice(&[0x80, 0x6b, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3]);
    packet.push(fu_indicator);
    packet.push(fu_header);
    packet.extend_from_slice(chunk);
    packet
}

fn measure<F: Fn(&[u8], u8, u8) -> Vec<u8>>(run: F, chunk: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(run(black_box(chunk), 0x7c, 0x85)).len();
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for size in [256usize, 1198] {
        let chunk = vec![0x55; size];
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(baseline, &chunk, 500_000));
            after.push(measure(optimized, &chunk, 500_000));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("chunk={size} baseline_us={before} optimized_us={after} gain={gain:.1}%");
        assert_eq!(baseline(&chunk, 0x7c, 0x85), optimized(&chunk, 0x7c, 0x85));
    }
}
