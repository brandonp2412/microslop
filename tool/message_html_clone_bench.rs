use std::hint::black_box;
use std::time::Instant;

fn baseline(html: &String) -> usize {
    let raw_html = html.clone();
    black_box(raw_html.as_str()).len()
}

fn optimized(html: &str) -> usize {
    black_box(html).len()
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
    for bytes in [256usize, 4096, 65536] {
        let html = format!("<p>{}</p>", "x".repeat(bytes));
        let iterations = (10_000_000 / bytes).max(500);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&html), iterations));
            after.push(measure(|| optimized(&html), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("bytes={bytes} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
