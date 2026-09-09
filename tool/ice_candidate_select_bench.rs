use std::hint::black_box;
use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

#[derive(Clone)]
struct Candidate {
    udp: bool,
    component: u8,
    priority: u32,
    address: String,
    port: u16,
}

fn baseline(candidates: &[Candidate]) -> Option<SocketAddr> {
    let mut udp = candidates
        .iter()
        .filter(|c| c.udp && c.component == 1)
        .collect::<Vec<_>>();
    udp.sort_by_key(|candidate| std::cmp::Reverse(candidate.priority));
    for candidate in udp {
        if let Ok(addr) = format!("{}:{}", candidate.address, candidate.port).parse() {
            return Some(addr);
        }
    }
    None
}

fn candidate(candidates: &[Candidate]) -> Option<SocketAddr> {
    let mut best = None;
    for candidate in candidates {
        if !candidate.udp
            || candidate.component != 1
            || best.is_some_and(|(priority, _)| candidate.priority <= priority)
        {
            continue;
        }
        if let Ok(ip) = candidate.address.parse::<IpAddr>() {
            best = Some((candidate.priority, SocketAddr::new(ip, candidate.port)));
        }
    }
    best.map(|(_, addr)| addr)
}

fn source() -> Vec<Candidate> {
    (0..1000)
        .map(|i| Candidate {
            udp: i % 5 != 0,
            component: if i % 7 == 0 { 2 } else { 1 },
            priority: ((i * 104729) % 1_000_000) as u32,
            address: if i % 113 == 0 {
                "invalid".into()
            } else {
                format!("10.{}.{}.{}", (i >> 16) & 255, (i >> 8) & 255, i & 255)
            },
            port: 20000 + (i % 30000) as u16,
        })
        .collect()
}

fn main() {
    let candidates = source();
    assert_eq!(baseline(&candidates), candidate(&candidates));
    for _ in 0..5000 {
        black_box(baseline(&candidates));
        black_box(candidate(&candidates));
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..51 {
        let start = Instant::now();
        for _ in 0..5000 {
            black_box(baseline(&candidates));
        }
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        for _ in 0..5000 {
            black_box(candidate(&candidates));
        }
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable();
    new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len() / 2];
    let p95 = |v: &[u128]| v[v.len() * 95 / 100];
    println!("sort p50={}us p95={}us", p50(&old), p95(&old));
    println!("scan p50={}us p95={}us", p50(&new), p95(&new));
    println!(
        "gain={:.1}%",
        (p50(&old) as f64 - p50(&new) as f64) * 100.0 / p50(&old) as f64
    );
}
