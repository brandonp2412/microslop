use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Chat {
    id: String,
    preview: Option<String>,
}

fn baseline(chats: &[Chat]) -> usize {
    let missing = chats
        .iter()
        .filter(|chat| chat.preview.as_deref().is_none_or(str::is_empty))
        .map(|chat| chat.id.clone())
        .collect::<Vec<_>>();
    let mut previews = missing
        .into_iter()
        .map(|id| (id, Some(String::from("preview"))))
        .collect::<HashMap<_, _>>();
    let mut hydrated = chats.to_vec();
    for chat in &mut hydrated {
        if let Some(preview) = previews.remove(&chat.id).flatten() {
            chat.preview = Some(preview);
        }
    }
    black_box(hydrated).len()
}

fn indexed(chats: &[Chat]) -> usize {
    let missing = chats
        .iter()
        .enumerate()
        .filter_map(|(index, chat)| {
            chat.preview
                .as_deref()
                .is_none_or(str::is_empty)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    let previews = missing
        .into_iter()
        .map(|index| (index, Some(String::from("preview"))))
        .collect::<Vec<_>>();
    let mut hydrated = chats.to_vec();
    for (index, preview) in previews {
        if let Some(preview) = preview {
            hydrated[index].preview = Some(preview);
        }
    }
    black_box(hydrated).len()
}

fn measure(run: fn(&[Chat]) -> usize, chats: &[Chat]) -> Duration {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..10_000 {
        checksum ^= run(black_box(chats));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let chats = (0..256)
        .map(|index| Chat {
            id: format!("19:{index:032x}@thread.v2"),
            preview: (index % 4 == 0).then(|| String::from("existing")),
        })
        .collect::<Vec<_>>();
    let mut before = (0..15).map(|_| measure(baseline, &chats)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(indexed, &chats)).collect::<Vec<_>>();
    let before_p50 = percentile(&mut before, 1, 2);
    let after_p50 = percentile(&mut after, 1, 2);
    let before_p95 = percentile(&mut before, 19, 20);
    let after_p95 = percentile(&mut after, 19, 20);
    println!(
        "p50_us={}->{} gain={:.1}% p95_us={}->{} gain={:.1}%",
        before_p50.as_micros(),
        after_p50.as_micros(),
        (before_p50.as_secs_f64() - after_p50.as_secs_f64()) * 100.0 / before_p50.as_secs_f64(),
        before_p95.as_micros(),
        after_p95.as_micros(),
        (before_p95.as_secs_f64() - after_p95.as_secs_f64()) * 100.0 / before_p95.as_secs_f64(),
    );
}
