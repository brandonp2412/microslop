use std::hint::black_box;
use std::time::Instant;

const FRAME_SAMPLES: usize = 160;
const TARGET_RATE: u32 = 8000;

fn frame(mut sample_index: u64) -> Vec<i16> {
    let mut frame = Vec::with_capacity(FRAME_SAMPLES);
    for _ in 0..FRAME_SAMPLES {
        let t = sample_index as f64 / TARGET_RATE as f64;
        let sample = ((std::f64::consts::TAU * 440.0 * t).sin()
            + (std::f64::consts::TAU * 480.0 * t).sin())
            * 0.15
            * i16::MAX as f64;
        frame.push(sample as i16);
        sample_index += 1;
    }
    frame
}

fn baseline() -> i64 {
    let mut total = 0i64;
    let mut sample_index = 0u64;
    for _ in 0..100 {
        let values = frame(sample_index);
        sample_index += FRAME_SAMPLES as u64;
        total += values.iter().map(|&v| i64::from(v)).sum::<i64>();
        black_box(values);
    }
    total
}

fn cached(frames: &[Vec<i16>]) -> i64 {
    let mut total = 0i64;
    for index in 0..100 {
        let values = frames[index % frames.len()].clone();
        total += values.iter().map(|&v| i64::from(v)).sum::<i64>();
        black_box(values);
    }
    total
}

fn main() {
    let frames = (0..5)
        .map(|index| frame((index * FRAME_SAMPLES) as u64))
        .collect::<Vec<_>>();
    assert_eq!(baseline(), cached(&frames));
    for _ in 0..20 {
        black_box(baseline());
        black_box(cached(&frames));
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..101 {
        let start = Instant::now();
        black_box(baseline());
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        black_box(cached(&frames));
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable();
    new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len() / 2];
    let p95 = |v: &[u128]| v[v.len() * 95 / 100];
    println!("baseline p50={}us p95={}us", p50(&old), p95(&old));
    println!("cached   p50={}us p95={}us", p50(&new), p95(&new));
    println!("p50 gain={:.1}%", (p50(&old) as f64 - p50(&new) as f64) * 100.0 / p50(&old) as f64);
}
