use std::hint::black_box;
use std::time::Instant;

fn html_escape(value: &str) -> String {
    if !value
        .as_bytes()
        .iter()
        .any(|byte| matches!(byte, b'&' | b'<' | b'>' | b'"' | b'\''))
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

fn baseline(value: &str) -> String {
    let mut result = String::new();
    let mut offset = 0;
    while offset < value.len() {
        let rest = &value[offset..];
        let Some(start) = [rest.find("https://"), rest.find("http://")]
            .into_iter()
            .flatten()
            .min()
        else {
            result.push_str(&html_escape(rest));
            break;
        };
        result.push_str(&rest[..start]);
        offset += start + 1;
    }
    result
}

fn optimized(value: &str) -> String {
    if !value.contains("https://") && !value.contains("http://") {
        return html_escape(value);
    }
    baseline(value)
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
    let ordinary = "Can you check the latest deployment and send the current status to the team? ".repeat(8);
    let escaped = "Can you check <this> & send the \"current\" status? ".repeat(8);
    for (name, input) in [("ordinary", ordinary.as_str()), ("escaped", escaped.as_str())] {
        let iterations = 200_000;
        let before = median((0..7).map(|_| measure(baseline, input, iterations)).collect());
        let after = median((0..7).map(|_| measure(optimized, input, iterations)).collect());
        println!("{name} baseline_us={before} optimized_us={after} gain={:.1}%", (before as f64 - after as f64) * 100.0 / before as f64);
    }
}
