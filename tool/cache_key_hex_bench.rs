use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Instant;

fn baseline(key: &str) -> String {
    key.as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn optimized(key: &str) -> String {
    let mut encoded = String::with_capacity(key.len() * 2);
    for byte in key.bytes() {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn measure<F: Fn(&str) -> String>(f: F, keys: &[String], rounds: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..rounds {
        for key in keys {
            checksum += black_box(f(black_box(key))).len();
        }
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let keys = (0..256)
        .map(|index| {
            format!("message-cache-{index:04}-19:meeting_abcdefghijklmnopqrstuvwxyz@thread.v2")
        })
        .collect::<Vec<_>>();
    let mut before = Vec::new();
    let mut after = Vec::new();
    for _ in 0..9 {
        before.push(measure(baseline, &keys, 500));
        after.push(measure(optimized, &keys, 500));
    }
    let before = median(before);
    let after = median(after);
    let gain = (before as f64 - after as f64) / before as f64 * 100.0;
    println!("baseline_us={before} optimized_us={after} gain={gain:.1}%");
    assert_eq!(baseline(&keys[0]), optimized(&keys[0]));
}
