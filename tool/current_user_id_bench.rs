use std::hint::black_box;
use std::time::Instant;

fn baseline(id: &String, messages: usize) -> usize {
    let current_user_id = Some(id.clone());
    (0..messages)
        .map(|_| current_user_id.clone().map_or(0, |id| black_box(id).len()))
        .sum()
}

fn optimized(id: &str, messages: usize) -> usize {
    let current_user_id = Some(id);
    (0..messages)
        .map(|_| current_user_id.map_or(0, |id| black_box(id).len()))
        .sum()
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
    let id = "00000000-0000-0000-0000-000000000001".to_owned();
    for messages in [20usize, 100, 1000] {
        let iterations = (100_000 / messages).max(100);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&id, messages), iterations));
            after.push(measure(|| optimized(&id, messages), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("messages={messages} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
