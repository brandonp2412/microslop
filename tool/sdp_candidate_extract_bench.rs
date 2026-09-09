use std::hint::black_box;
use std::time::Instant;

fn baseline(line: &str) -> Option<(&str, u16)> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    (parts.len() >= 6).then(|| (parts[4], parts[5].parse().unwrap_or(0)))
}

fn direct(line: &str) -> Option<(&str, u16)> {
    let mut parts = line.split_whitespace();
    let address = parts.nth(4)?;
    Some((address, parts.next()?.parse().unwrap_or(0)))
}

fn run(lines: &[String], direct_mode: bool) -> u128 {
    let start = Instant::now();
    for _ in 0..20_000 {
        for line in lines {
            black_box(if direct_mode {
                direct(line)
            } else {
                baseline(line)
            });
        }
    }
    start.elapsed().as_micros()
}

fn percentile(values: &mut [u128], n: usize) -> u128 {
    values.sort_unstable();
    values[values.len() * n / 100]
}

fn main() {
    let lines = (0..100)
        .map(|i| {
            format!(
                "a=candidate:{i} 1 UDP 2122260223 192.168.1.{} {} typ host generation 0",
                i % 250,
                50_000 + i
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(baseline(&lines[0]), direct(&lines[0]));
    for _ in 0..4 {
        black_box(run(&lines, false));
        black_box(run(&lines, true));
    }
    let mut old = (0..15).map(|_| run(&lines, false)).collect::<Vec<_>>();
    let mut new = (0..15).map(|_| run(&lines, true)).collect::<Vec<_>>();
    let old_p50 = percentile(&mut old, 50);
    let new_p50 = percentile(&mut new, 50);
    let old_p95 = percentile(&mut old, 95);
    let new_p95 = percentile(&mut new, 95);
    println!(
        "p50_us={old_p50}->{new_p50} gain={:.1}%",
        (old_p50 as f64 - new_p50 as f64) * 100.0 / old_p50 as f64
    );
    println!(
        "p95_us={old_p95}->{new_p95} gain={:.1}%",
        (old_p95 as f64 - new_p95 as f64) * 100.0 / old_p95 as f64
    );
}
