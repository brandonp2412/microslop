use std::hint::black_box;
use std::time::Instant;

fn baseline(mut pending: Vec<u8>) -> usize {
    let mut bytes = 0;
    while let Some(pos) = pending.iter().position(|&b| b == b'\n') {
        let line: Vec<u8> = pending.drain(..=pos).collect();
        bytes += line.len() - 1;
    }
    black_box(bytes + pending.len())
}

fn optimized(mut pending: Vec<u8>) -> usize {
    let Some(last_newline) = pending.iter().rposition(|&b| b == b'\n') else {
        return black_box(pending.len());
    };
    let complete = pending.drain(..=last_newline).collect::<Vec<_>>();
    let bytes = complete
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
        .map(<[u8]>::len)
        .sum::<usize>();
    black_box(bytes + pending.len())
}

fn measure<F: Fn() -> usize>(run: F, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum += black_box(run());
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for lines in [10usize, 100, 500] {
        let input = (0..lines)
            .flat_map(|index| format!("2026-09-09 INFO message number {index:04} with payload\n").into_bytes())
            .collect::<Vec<_>>();
        let iterations = 20_000 / lines;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(input.clone()), iterations));
            after.push(measure(|| optimized(input.clone()), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("lines={lines} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
