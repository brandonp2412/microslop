use std::hint::black_box;
use std::sync::OnceLock;
use std::time::Instant;

fn generate() -> Vec<Vec<u8>> {
    let sps = vec![0x67, 0x42, 0xC0, 0x0C, 0xDA, 0x0F, 0x0A, 0x68];
    let pps = vec![0x68, 0xCE, 0x38, 0x80];
    let mut idr = vec![0x65, 0x88, 0x80, 0x40];
    idr.extend(std::iter::repeat_n(0xFF, 25));
    idr.push(0x80);
    vec![sps, pps, idr]
}

static FRAME: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
fn cached() -> &'static [Vec<u8>] {
    FRAME.get_or_init(generate)
}

fn main() {
    for _ in 0..10000 { black_box(generate()); black_box(cached()); }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..31 {
        let start = Instant::now();
        for _ in 0..1_000_000 { black_box(generate()); }
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        for _ in 0..1_000_000 { black_box(cached()); }
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable(); new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len()/2];
    let p95 = |v: &[u128]| v[v.len()*95/100];
    println!("generate p50={}us p95={}us", p50(&old), p95(&old));
    println!("cached   p50={}us p95={}us", p50(&new), p95(&new));
    println!("gain={:.1}%", (p50(&old) as f64-p50(&new) as f64)*100.0/p50(&old) as f64);
}
