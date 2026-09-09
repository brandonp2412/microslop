use std::collections::HashSet;
use std::hint::black_box;
use std::time::Instant;

fn baseline(lines: &[String], kind: &str, remote: &HashSet<String>, local: &[String]) -> usize {
    let found = lines
        .iter()
        .find(|line| line.starts_with(&format!("m={kind} ")))
        .is_some() as usize;
    found
        + local
            .iter()
            .filter(|payload| remote.iter().any(|candidate| candidate == *payload))
            .count()
}

fn direct(lines: &[String], kind: &str, remote: &HashSet<String>, local: &[String]) -> usize {
    let prefix = format!("m={kind} ");
    let found = lines
        .iter()
        .find(|line| line.starts_with(&prefix))
        .is_some() as usize;
    found
        + local
            .iter()
            .filter(|payload| remote.contains(payload.as_str()))
            .count()
}

fn run(lines: &[String], remote: &HashSet<String>, local: &[String], direct_mode: bool) -> u128 {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..20_000 {
        checksum += if direct_mode {
            direct(lines, "video", remote, local)
        } else {
            baseline(lines, "video", remote, local)
        };
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn percentile(values: &mut [u128], n: usize) -> u128 {
    values.sort_unstable();
    values[values.len() * n / 100]
}

fn main() {
    let mut lines = (0..250)
        .map(|i| format!("a=x-test:{i}"))
        .collect::<Vec<_>>();
    lines.push("m=video 9 RTP/SAVP 96 97 98".to_owned());
    let remote = (64..192).map(|i| i.to_string()).collect::<HashSet<_>>();
    let local = (0..256).map(|i| i.to_string()).collect::<Vec<_>>();
    assert_eq!(
        baseline(&lines, "video", &remote, &local),
        direct(&lines, "video", &remote, &local)
    );
    for _ in 0..5 {
        black_box(run(&lines, &remote, &local, false));
        black_box(run(&lines, &remote, &local, true));
    }
    let mut old = (0..15)
        .map(|_| run(&lines, &remote, &local, false))
        .collect::<Vec<_>>();
    let mut new = (0..15)
        .map(|_| run(&lines, &remote, &local, true))
        .collect::<Vec<_>>();
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
