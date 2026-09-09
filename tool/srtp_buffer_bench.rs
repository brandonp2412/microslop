use std::hint::black_box;
use std::time::Instant;

const HEADER: usize = 12;
const TAG: usize = 10;

fn baseline(packet: &[u8]) -> Vec<u8> {
    let header = &packet[..HEADER];
    let payload = &packet[HEADER..];
    let mut encrypted = payload.to_vec();
    for byte in &mut encrypted {
        *byte ^= 0x5a;
    }
    let mut output = Vec::with_capacity(HEADER + encrypted.len() + TAG);
    output.extend_from_slice(header);
    output.extend_from_slice(&encrypted);
    output.resize(output.len() + TAG, 0);
    output
}

fn optimized(packet: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(packet.len() + TAG);
    output.extend_from_slice(packet);
    for byte in &mut output[HEADER..] {
        *byte ^= 0x5a;
    }
    output.resize(output.len() + TAG, 0);
    output
}

fn measure<F: Fn(&[u8]) -> Vec<u8>>(run: F, packet: &[u8], iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(run(black_box(packet))).len();
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn tag_vec(input: &[u8; 20]) -> Vec<u8> {
    input[..TAG].to_vec()
}

fn tag_array(input: &[u8; 20]) -> [u8; TAG] {
    let mut tag = [0; TAG];
    tag.copy_from_slice(&input[..TAG]);
    tag
}

fn measure_tag_vec(input: &[u8; 20], iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(tag_vec(black_box(input)))[0] as usize;
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn measure_tag_array(input: &[u8; 20], iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(tag_array(black_box(input)))[0] as usize;
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for size in [172usize, 1200, 4096] {
        let packet = (0..size).map(|index| index as u8).collect::<Vec<_>>();
        let iterations = 200_000 * 1200 / size;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(baseline, &packet, iterations));
            after.push(measure(optimized, &packet, iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("bytes={size} baseline_us={before} optimized_us={after} gain={gain:.1}%");
        assert_eq!(baseline(&packet), optimized(&packet));
    }

    let input = [0x5a; 20];
    let mut before = Vec::new();
    let mut after = Vec::new();
    for _ in 0..9 {
        before.push(measure_tag_vec(&input, 5_000_000));
        after.push(measure_tag_array(&input, 5_000_000));
    }
    let before = median(before);
    let after = median(after);
    let gain = (before as f64 - after as f64) / before as f64 * 100.0;
    println!("auth_tag baseline_us={before} optimized_us={after} gain={gain:.1}%");
    assert_eq!(tag_vec(&input).as_slice(), tag_array(&input).as_slice());
}
