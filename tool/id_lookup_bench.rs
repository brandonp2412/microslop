use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

fn ids_equal(left: &str, right: &str) -> bool {
    left.trim_matches(['{', '}'])
        .eq_ignore_ascii_case(right.trim_matches(['{', '}']))
}

fn key(id: &str) -> String {
    id.trim_matches(['{', '}']).to_ascii_lowercase()
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
    for count in [10usize, 100, 1000] {
        let entries = (0..count)
            .map(|index| {
                (
                    format!("{{AAAAAAAA-BBBB-CCCC-DDDD-{index:012X}}}"),
                    format!("User {index}"),
                )
            })
            .collect::<Vec<_>>();
        let map = entries
            .iter()
            .map(|(id, name)| (key(id), name.clone()))
            .collect::<HashMap<_, _>>();
        let needle = format!("aaaaaaaa-bbbb-cccc-dddd-{:012x}", count - 1);
        let iterations = 100_000 / count;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(
                || {
                    entries
                        .iter()
                        .find(|(id, _)| ids_equal(id, &needle))
                        .map_or(0, |(_, name)| name.len())
                },
                iterations,
            ));
            after.push(measure(
                || map.get(&key(&needle)).map_or(0, String::len),
                iterations,
            ));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("entries={count} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
