use std::collections::HashSet;
use std::hint::black_box;
use std::time::Instant;

const LIMIT: usize = 12;

fn baseline(channels: &[String], chats: &[String]) -> Vec<String> {
    let channel_ids = channels.iter().map(|id| id.clone());
    let chat_ids = chats.iter().map(|id| id.clone());
    let mut seen = HashSet::new();
    channel_ids
        .map(|id| id.as_str().to_string())
        .chain(chat_ids.map(|id| id.as_str().to_string()))
        .filter_map(|id| (seen.insert(id.clone()) && !id.is_empty()).then_some(id))
        .take(LIMIT)
        .collect()
}

fn optimized(channels: &[String], chats: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    channels
        .iter()
        .map(String::as_str)
        .chain(chats.iter().map(String::as_str))
        .filter(|id| !id.is_empty() && seen.insert(*id))
        .take(LIMIT)
        .map(str::to_owned)
        .collect()
}

fn measure(mut run: impl FnMut() -> Vec<String>, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(run()).len();
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let channels = (0..2_000)
        .map(|i| format!("19:channel-{i}-with-a-realistic-identifier@thread.tacv2"))
        .collect::<Vec<_>>();
    let chats = (0..10_000)
        .map(|i| format!("19:chat-{i}-with-a-realistic-identifier@thread.v2"))
        .collect::<Vec<_>>();
    let iterations = 100_000;
    let before = median(
        (0..9)
            .map(|_| measure(|| baseline(&channels, &chats), iterations))
            .collect(),
    );
    let after = median(
        (0..9)
            .map(|_| measure(|| optimized(&channels, &chats), iterations))
            .collect(),
    );
    println!(
        "baseline_us={before} optimized_us={after} gain={:.1}%",
        (before - after) as f64 * 100.0 / before as f64
    );
}
