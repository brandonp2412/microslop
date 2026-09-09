use std::hint::black_box;
use std::time::Instant;

fn baseline(nals: &[Vec<u8>]) -> Vec<u8> {
    let mut annexb = Vec::new();
    for nal in nals {
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(nal);
    }
    annexb
}

fn reserved(nals: &[Vec<u8>]) -> Vec<u8> {
    let mut annexb = Vec::with_capacity(nals.iter().map(|nal| nal.len() + 4).sum());
    for nal in nals {
        annexb.extend_from_slice(&[0, 0, 0, 1]);
        annexb.extend_from_slice(nal);
    }
    annexb
}

fn main() {
    let nals = vec![
        vec![0x67; 32],
        vec![0x68; 12],
        vec![0x65; 140_000],
        vec![0x41; 18_000],
    ];
    assert_eq!(baseline(&nals), reserved(&nals));
    for _ in 0..100 {
        black_box(baseline(&nals));
        black_box(reserved(&nals));
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..301 {
        let start = Instant::now();
        black_box(baseline(&nals));
        old.push(start.elapsed().as_nanos());
        let start = Instant::now();
        black_box(reserved(&nals));
        new.push(start.elapsed().as_nanos());
    }
    old.sort_unstable();
    new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len() / 2];
    let p95 = |v: &[u128]| v[v.len() * 95 / 100];
    println!("baseline p50={}ns p95={}ns", p50(&old), p95(&old));
    println!("reserved p50={}ns p95={}ns", p50(&new), p95(&new));
    println!("p50 gain={:.1}%", (p50(&old) as f64 - p50(&new) as f64) * 100.0 / p50(&old) as f64);
}
