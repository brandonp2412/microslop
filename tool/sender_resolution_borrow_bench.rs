use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

struct Message {
    id: String,
    sender: String,
    sender_id: Option<String>,
}

fn cloned(messages: &[Message]) -> usize {
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| !message.id.is_empty())
        .map(|(index, message)| (index, message.id.clone(), message.sender_id.clone()))
        .map(|(index, id, sender_id)| index + id.len() + sender_id.as_deref().map_or(0, str::len))
        .sum()
}

fn resolved_baseline(messages: &[Message]) -> usize {
    let known_names = messages
        .iter()
        .filter_map(|message| {
            let sender_id = message.sender_id.as_deref()?;
            (!message.sender.is_empty()).then(|| (sender_id.to_ascii_lowercase(), message.sender.clone()))
        })
        .collect::<HashMap<_, _>>();
    let unresolved = messages
        .iter()
        .filter(|message| message.sender.is_empty() && !message.id.is_empty())
        .count();
    known_names.len() + unresolved
}

fn resolved_optimized(messages: &[Message]) -> usize {
    let unresolved = messages
        .iter()
        .filter(|message| message.sender.is_empty() && !message.id.is_empty())
        .count();
    if unresolved == 0 {
        return unresolved;
    }
    messages
        .iter()
        .filter_map(|message| {
            let sender_id = message.sender_id.as_deref()?;
            (!message.sender.is_empty()).then(|| (sender_id.to_ascii_lowercase(), message.sender.clone()))
        })
        .collect::<HashMap<_, _>>()
        .len()
}

fn borrowed(messages: &[Message]) -> usize {
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| !message.id.is_empty())
        .map(|(index, _)| index)
        .map(|index| {
            let message = &messages[index];
            index + message.id.len() + message.sender_id.as_deref().map_or(0, str::len)
        })
        .sum()
}

fn measure(run: fn(&[Message]) -> usize, messages: &[Message], iterations: usize) -> Duration {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum ^= black_box(run(black_box(messages)));
    }
    black_box(checksum);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let messages = (0..512)
        .map(|index| Message {
            id: format!("1741546710123-{index:032x}@thread.tacv2"),
            sender: "Resolved User".to_owned(),
            sender_id: Some(format!("8:orgid:{index:032x}")),
        })
        .collect::<Vec<_>>();
    let mut before = (0..15)
        .map(|_| measure(cloned, &messages, 10_000))
        .collect::<Vec<_>>();
    let mut after = (0..15)
        .map(|_| measure(borrowed, &messages, 10_000))
        .collect::<Vec<_>>();
    let before_p50 = percentile(&mut before, 1, 2);
    let after_p50 = percentile(&mut after, 1, 2);
    let before_p95 = percentile(&mut before, 19, 20);
    let after_p95 = percentile(&mut after, 19, 20);
    println!(
        "borrow p50_us={}->{} gain={:.1}% p95_us={}->{} gain={:.1}%",
        before_p50.as_micros(),
        after_p50.as_micros(),
        (before_p50.as_secs_f64() - after_p50.as_secs_f64()) * 100.0 / before_p50.as_secs_f64(),
        before_p95.as_micros(),
        after_p95.as_micros(),
        (before_p95.as_secs_f64() - after_p95.as_secs_f64()) * 100.0 / before_p95.as_secs_f64(),
    );

    let mut resolved_before = (0..15)
        .map(|_| measure(resolved_baseline, &messages, 10_000))
        .collect::<Vec<_>>();
    let mut resolved_after = (0..15)
        .map(|_| measure(resolved_optimized, &messages, 10_000))
        .collect::<Vec<_>>();
    let before_p50 = percentile(&mut resolved_before, 1, 2);
    let after_p50 = percentile(&mut resolved_after, 1, 2);
    let before_p95 = percentile(&mut resolved_before, 19, 20);
    let after_p95 = percentile(&mut resolved_after, 19, 20);
    println!(
        "resolved p50_us={}->{} gain={:.1}% p95_us={}->{} gain={:.1}%",
        before_p50.as_micros(),
        after_p50.as_micros(),
        (before_p50.as_secs_f64() - after_p50.as_secs_f64()) * 100.0 / before_p50.as_secs_f64(),
        before_p95.as_micros(),
        after_p95.as_micros(),
        (before_p95.as_secs_f64() - after_p95.as_secs_f64()) * 100.0 / before_p95.as_secs_f64(),
    );
}
