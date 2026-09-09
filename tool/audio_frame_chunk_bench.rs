use std::hint::black_box;
use std::time::Instant;

fn downmix(samples: &[i16], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let mut energy = vec![0u64; channels];
    for frame in samples.chunks_exact(channels) {
        for (channel, &sample) in frame.iter().enumerate() {
            energy[channel] += i64::from(sample).unsigned_abs();
        }
    }
    let channel = energy
        .iter()
        .enumerate()
        .max_by_key(|(_, energy)| *energy)
        .map(|(channel, _)| channel)
        .unwrap_or(0);
    samples
        .chunks_exact(channels)
        .map(|frame| frame[channel])
        .collect()
}

fn baseline(samples: &mut Vec<i16>, frame_samples: usize, channels: usize) -> usize {
    let chunk: Vec<i16> = samples.drain(..frame_samples).collect();
    downmix(&chunk, channels).len()
}

fn optimized_downmix(samples: &[i16], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return samples.to_vec();
    }
    if channels == 2 {
        let (mut left, mut right) = (0u64, 0u64);
        for frame in samples.chunks_exact(2) {
            left += i64::from(frame[0]).unsigned_abs();
            right += i64::from(frame[1]).unsigned_abs();
        }
        let channel = usize::from(right > left);
        return samples
            .chunks_exact(2)
            .map(|frame| frame[channel])
            .collect();
    }
    downmix(samples, channels)
}

fn optimized(samples: &mut Vec<i16>, frame_samples: usize, channels: usize) -> usize {
    let mono = optimized_downmix(&samples[..frame_samples], channels);
    samples.drain(..frame_samples);
    mono.len()
}

fn measure(
    run: fn(&mut Vec<i16>, usize, usize) -> usize,
    source: &[i16],
    frame_samples: usize,
    channels: usize,
    iterations: usize,
) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        let mut samples = source.to_vec();
        checksum = checksum.wrapping_add(black_box(run(
            &mut samples,
            frame_samples,
            channels,
        )));
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for channels in [1, 2] {
        let frame_samples = 960 * channels;
        let source = (0..frame_samples * 2)
            .map(|i| ((i as f64 * 0.07).sin() * 12000.0) as i16)
            .collect::<Vec<_>>();
        let iterations = 100_000;
        let before = median(
            (0..9)
                .map(|_| measure(baseline, &source, frame_samples, channels, iterations))
                .collect(),
        );
        let after = median(
            (0..9)
                .map(|_| measure(optimized, &source, frame_samples, channels, iterations))
                .collect(),
        );
        println!(
            "channels={channels} baseline_us={before} optimized_us={after} gain={:.1}%",
            (before as f64 - after as f64) * 100.0 / before as f64
        );
    }
}
