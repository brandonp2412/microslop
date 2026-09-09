use std::hint::black_box;
use std::time::Instant;

fn baseline(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn optimized(value: &str) -> String {
    if !value
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'&' | b'<' | b'>' | b'\"' | b'\''))
    {
        return value.to_owned();
    }
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn measure(run: fn(&str) -> String, input: &str, iterations: usize) -> u128 {
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(run(black_box(input)));
    }
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let ordinary = "Can you check the deployment status and send me the latest update? ".repeat(8);
    let markup = "Use <strong>this</strong> & say \"hello\" to O'Brien > team. ".repeat(8);
    for (name, input) in [("ordinary", ordinary.as_str()), ("markup", markup.as_str())] {
        let iterations = 200_000;
        let before = median((0..7).map(|_| measure(baseline, input, iterations)).collect());
        let after = median((0..7).map(|_| measure(optimized, input, iterations)).collect());
        println!("{name} baseline_us={before} optimized_us={after} gain={:.1}%", (before as f64 - after as f64) * 100.0 / before as f64);
    }
}
