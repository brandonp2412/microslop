use std::hint::black_box;
use std::time::Instant;

fn kernel(source_rate: u32, target_rate: u32) -> (Vec<f64>, f64) {
    let ratio = source_rate as f64 / target_rate as f64;
    let cutoff = 0.45 / ratio;
    let mut values = Vec::with_capacity(65);
    let mut weight = 0.0;
    for offset in -32..=32 {
        let distance = -(offset as f64);
        let phase = 2.0 * std::f64::consts::PI * cutoff * distance;
        let sinc = if phase.abs() < 1e-9 { 1.0 } else { phase.sin() / phase };
        let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / 32.0).cos();
        let coefficient = sinc * window;
        values.push(coefficient);
        weight += coefficient;
    }
    (values, weight)
}

fn baseline(samples: &[i16], position: u64, source_rate: u32, target_rate: u32) -> i16 {
    let ratio = source_rate as f64 / target_rate as f64;
    let center = position as f64 / target_rate as f64 - 32.0;
    let cutoff = 0.45 / ratio;
    let mut sum = 0.0;
    let mut weight = 0.0;
    for index in (center - 32.0).ceil() as isize..=(center + 32.0).floor() as isize {
        let distance = center - index as f64;
        let phase = 2.0 * std::f64::consts::PI * cutoff * distance;
        let sinc = if phase.abs() < 1e-9 { 1.0 } else { phase.sin() / phase };
        let window = 0.5 + 0.5 * (std::f64::consts::PI * distance / 32.0).cos();
        let coefficient = sinc * window;
        let sample = usize::try_from(index).ok().and_then(|i| samples.get(i)).copied().unwrap_or(0);
        sum += f64::from(sample) * coefficient;
        weight += coefficient;
    }
    (sum / weight).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
}

fn cached(samples: &[i16], position: u64, target_rate: u32, coefficients: &[f64], weight: f64) -> i16 {
    let center = (position / u64::from(target_rate)) as isize - 32;
    let mut sum = 0.0;
    for (offset, coefficient) in coefficients.iter().enumerate() {
        let index = center - 32 + offset as isize;
        let sample = usize::try_from(index).ok().and_then(|i| samples.get(i)).copied().unwrap_or(0);
        sum += f64::from(sample) * coefficient;
    }
    (sum / weight).round().clamp(i16::MIN as f64, i16::MAX as f64) as i16
}

fn main() {
    const SOURCE: u32 = 48000;
    const TARGET: u32 = 8000;
    let samples = (0..2000).map(|i| (((i * 193) % 60000) as i32 - 30000) as i16).collect::<Vec<_>>();
    let (coefficients, weight) = kernel(SOURCE, TARGET);
    for output in 0..300u64 {
        let position = output * u64::from(SOURCE);
        assert_eq!(baseline(&samples, position, SOURCE, TARGET), cached(&samples, position, TARGET, &coefficients, weight));
    }
    for _ in 0..10 {
        for output in 0..160u64 {
            black_box(baseline(&samples, output * u64::from(SOURCE), SOURCE, TARGET));
            black_box(cached(&samples, output * u64::from(SOURCE), TARGET, &coefficients, weight));
        }
    }
    let mut old = Vec::new();
    let mut new = Vec::new();
    for _ in 0..101 {
        let start = Instant::now();
        for output in 0..160u64 {
            black_box(baseline(&samples, output * u64::from(SOURCE), SOURCE, TARGET));
        }
        old.push(start.elapsed().as_micros());
        let start = Instant::now();
        for output in 0..160u64 {
            black_box(cached(&samples, output * u64::from(SOURCE), TARGET, &coefficients, weight));
        }
        new.push(start.elapsed().as_micros());
    }
    old.sort_unstable(); new.sort_unstable();
    let p50 = |v: &[u128]| v[v.len()/2];
    let p95 = |v: &[u128]| v[v.len()*95/100];
    println!("baseline p50={}us p95={}us", p50(&old), p95(&old));
    println!("cached   p50={}us p95={}us", p50(&new), p95(&new));
    println!("p50 gain={:.1}%", (p50(&old) as f64-p50(&new) as f64)*100.0/p50(&old) as f64);
}
