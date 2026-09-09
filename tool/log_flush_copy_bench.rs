use std::hint::black_box;
use std::time::Instant;

fn baseline(pending: &mut Vec<u8>) -> Vec<String> {
    let Some(last_newline) = pending.iter().rposition(|&b| b == b'\n') else { return vec![]; };
    let complete = pending.drain(..=last_newline).collect::<Vec<_>>();
    complete[..complete.len() - 1]
        .split(|&b| b == b'\n')
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect()
}

fn candidate(pending: &mut Vec<u8>) -> Vec<String> {
    let Some(last_newline) = pending.iter().rposition(|&b| b == b'\n') else { return vec![]; };
    let lines = pending[..last_newline]
        .split(|&b| b == b'\n')
        .map(|line| String::from_utf8_lossy(line).into_owned())
        .collect();
    pending.drain(..=last_newline);
    lines
}

fn input() -> Vec<u8> {
    let mut bytes = Vec::new();
    for i in 0..500 {
        bytes.extend_from_slice(format!("2026-09-09T06:00:00 INFO call packet={i} payload={}\n", "x".repeat(80)).as_bytes());
    }
    bytes.extend_from_slice(b"partial trailing log message");
    bytes
}

fn run(f: fn(&mut Vec<u8>) -> Vec<String>) -> u128 {
    let source = input();
    let start = Instant::now();
    let mut total = 0usize;
    for _ in 0..2000 {
        let mut pending = source.clone();
        let lines = f(&mut pending);
        total += lines.len() + pending.len();
        black_box(lines);
        black_box(pending);
    }
    black_box(total);
    start.elapsed().as_micros()
}

fn main() {
    let mut a = input(); let mut b = a.clone();
    assert_eq!(baseline(&mut a), candidate(&mut b)); assert_eq!(a, b);
    for _ in 0..10 { black_box(run(baseline)); black_box(run(candidate)); }
    let mut old = Vec::new(); let mut new = Vec::new();
    for _ in 0..31 { old.push(run(baseline)); new.push(run(candidate)); }
    old.sort_unstable(); new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len()/2];
    let p95 = |v: &[u128]| v[v.len()*95/100];
    println!("baseline p50={}us p95={}us", p50(&old), p95(&old));
    println!("no-copy  p50={}us p95={}us", p50(&new), p95(&new));
    println!("gain={:.1}%", (p50(&old) as f64-p50(&new) as f64)*100.0/p50(&old) as f64);
}
