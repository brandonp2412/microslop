use std::hint::black_box;
use std::time::Instant;

fn baseline(sample: i16) -> u8 {
    const BIAS: i16 = 0x84;
    const CLIP: i16 = 32635;
    let sign: i16;
    let mut mag: i16;
    if sample < 0 {
        mag = if sample == i16::MIN { CLIP } else { -sample };
        sign = 0x80;
    } else {
        mag = sample;
        sign = 0;
    }
    if mag > CLIP {
        mag = CLIP;
    }
    mag += BIAS;
    let mut exponent: u8 = 7;
    let mut exp_mask: i16 = 0x4000;
    while exponent > 0 && (mag & exp_mask) == 0 {
        exponent -= 1;
        exp_mask >>= 1;
    }
    let mantissa = ((mag >> (exponent as i16 + 3)) & 0x0F) as u8;
    !((sign as u8) | (exponent << 4) | mantissa)
}

fn optimized(sample: i16) -> u8 {
    const BIAS: u16 = 0x84;
    const CLIP: i32 = 32635;
    let sign = if sample < 0 { 0x80 } else { 0 };
    let magnitude = i32::from(sample).unsigned_abs().min(CLIP as u32) as u16 + BIAS;
    let exponent = (15 - magnitude.leading_zeros() as u8).saturating_sub(7);
    let mantissa = ((magnitude >> (exponent + 3)) & 0x0F) as u8;
    !(sign | (exponent << 4) | mantissa)
}

fn measure(run: fn(i16) -> u8, samples: &[i16], rounds: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0u8;
    for _ in 0..rounds {
        for &sample in samples {
            checksum = checksum.wrapping_add(black_box(run(black_box(sample))));
        }
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for raw in u16::MIN..=u16::MAX {
        let sample = raw as i16;
        assert_eq!(baseline(sample), optimized(sample), "sample={sample}");
    }
    let samples = (0..8000)
        .map(|index| ((index as f64 * 0.091).sin() * 30000.0) as i16)
        .collect::<Vec<_>>();
    let rounds = 2000;
    let before = median((0..9).map(|_| measure(baseline, &samples, rounds)).collect());
    let after = median((0..9).map(|_| measure(optimized, &samples, rounds)).collect());
    println!("baseline_us={before} optimized_us={after} gain={:.1}%", (before as f64 - after as f64) * 100.0 / before as f64);
}
