use std::hint::black_box;
use std::time::Instant;

fn baseline(input: &[f32], output: &mut Vec<i16>) {
    let converted: Vec<i16> = input
        .iter()
        .map(|&sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect();
    output.extend_from_slice(&converted);
}

fn optimized(input: &[f32], output: &mut Vec<i16>) {
    output.extend(
        input
            .iter()
            .map(|&sample| (sample.clamp(-1.0, 1.0) * 32767.0) as i16),
    );
}

fn measure(run: fn(&[f32], &mut Vec<i16>), input: &[f32], iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let mut output = Vec::with_capacity(input.len());
        run(black_box(input), &mut output);
        checksum = checksum.wrapping_add(black_box(output.len()));
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for samples in [160, 480, 960, 1920] {
        let input = (0..samples)
            .map(|i| ((i as f32 * 0.013).sin() * 1.2).clamp(-1.2, 1.2))
            .collect::<Vec<_>>();
        let iterations = 200_000;
        let before = median(
            (0..9)
                .map(|_| measure(baseline, &input, iterations))
                .collect(),
        );
        let after = median(
            (0..9)
                .map(|_| measure(optimized, &input, iterations))
                .collect(),
        );
        println!(
            "samples={samples} baseline_us={before} optimized_us={after} gain={:.1}%",
            (before - after) as f64 * 100.0 / before as f64
        );
    }
}
