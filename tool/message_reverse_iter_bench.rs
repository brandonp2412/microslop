use std::hint::black_box;
use std::time::Instant;

fn baseline(messages: &[String]) -> usize {
    let mut refs: Vec<&String> = messages.iter().collect();
    refs.reverse();
    refs.iter().map(|message| message.len()).sum()
}

fn optimized(messages: &[String]) -> usize {
    messages.iter().rev().map(|message| message.len()).sum()
}

fn measure(mut run: impl FnMut() -> usize, iterations: usize) -> u128 {
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
    for count in [100, 1_000, 10_000] {
        let messages = (0..count)
            .map(|i| format!("message-{i}-with-representative-content"))
            .collect::<Vec<_>>();
        let iterations = 10_000_000 / count;
        let before = median(
            (0..9)
                .map(|_| measure(|| baseline(&messages), iterations))
                .collect(),
        );
        let after = median(
            (0..9)
                .map(|_| measure(|| optimized(&messages), iterations))
                .collect(),
        );
        println!(
            "messages={count} baseline_us={before} optimized_us={after} gain={:.1}%",
            (before - after) as f64 * 100.0 / before as f64
        );
    }
}
