use std::hint::black_box;
use std::time::Instant;

fn baseline(line: &str) -> Option<(u8, u32, &str, u16, &str)> {
    let content = line
        .strip_prefix("a=candidate:")
        .or_else(|| line.strip_prefix("candidate:"))?;
    let parts = content.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 8 || parts[6] != "typ" {
        return None;
    }
    Some((
        parts[1].parse().ok()?,
        parts[3].parse().ok()?,
        parts[4],
        parts[5].parse().ok()?,
        parts[7],
    ))
}

fn direct(line: &str) -> Option<(u8, u32, &str, u16, &str)> {
    let content = line
        .strip_prefix("a=candidate:")
        .or_else(|| line.strip_prefix("candidate:"))?;
    let mut parts = content.split_whitespace();
    parts.next()?;
    let component = parts.next()?.parse().ok()?;
    parts.next()?;
    let priority = parts.next()?.parse().ok()?;
    let address = parts.next()?;
    let port = parts.next()?.parse().ok()?;
    if parts.next()? != "typ" {
        return None;
    }
    Some((component, priority, address, port, parts.next()?))
}

fn source() -> Vec<String> {
    (0..1000)
        .map(|i| {
            format!(
                "a=candidate:{i} 1 UDP {} 10.{}.{}.{} {} typ host generation 0 network-id 1",
                2_000_000_000u32 - i,
                (i >> 16) & 255,
                (i >> 8) & 255,
                i & 255,
                20_000 + i
            )
        })
        .collect()
}

fn run(lines: &[String], direct_mode: bool) -> u128 {
    let start = Instant::now();
    for _ in 0..5000 {
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
    let lines = source();
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
