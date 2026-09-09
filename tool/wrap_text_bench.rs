use std::hint::black_box;
use std::time::Instant;

fn baseline(text: &str, max_width: usize) -> usize {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.len() <= max_width {
            result.push(line.to_string());
        } else {
            let words: Vec<&str> = line.split_whitespace().collect();
            let mut current = String::new();
            for word in words {
                if current.is_empty() {
                    current = word.to_string();
                } else if current.len() + 1 + word.len() <= max_width {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    result.push(current);
                    current = word.to_string();
                }
            }
            if !current.is_empty() {
                result.push(current);
            }
        }
    }
    black_box(result.iter().map(String::len).sum())
}

fn optimized(text: &str, max_width: usize) -> usize {
    let mut result = Vec::new();
    for line in text.lines() {
        if line.len() <= max_width {
            result.push(line.to_string());
        } else {
            let mut current = String::new();
            for word in line.split_whitespace() {
                if current.is_empty() {
                    current = word.to_string();
                } else if current.len() + 1 + word.len() <= max_width {
                    current.push(' ');
                    current.push_str(word);
                } else {
                    result.push(current);
                    current = word.to_string();
                }
            }
            if !current.is_empty() {
                result.push(current);
            }
        }
    }
    black_box(result.iter().map(String::len).sum())
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
    for words in [20usize, 100, 500] {
        let text = (0..words)
            .map(|index| format!("word{index}"))
            .collect::<Vec<_>>()
            .join(" ");
        let iterations = 20_000 / words;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&text, 60), iterations));
            after.push(measure(|| optimized(&text, 60), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("words={words} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
