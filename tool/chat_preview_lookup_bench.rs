use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone)]
struct Chat {
    id: String,
    preview: Option<String>,
}

fn baseline(mut chats: Vec<Chat>, previews: &[(String, Option<String>)]) -> usize {
    for (chat_id, preview) in previews {
        if let Some(preview) = preview
            && let Some(chat) = chats.iter_mut().find(|chat| chat.id == *chat_id)
        {
            chat.preview = Some(preview.clone());
        }
    }
    chats.iter().filter(|chat| chat.preview.is_some()).count()
}

fn optimized(mut chats: Vec<Chat>, previews: &[(String, Option<String>)]) -> usize {
    let previews = previews
        .iter()
        .filter_map(|(id, preview)| preview.as_ref().map(|preview| (id.as_str(), preview)))
        .collect::<HashMap<_, _>>();
    for chat in &mut chats {
        if let Some(preview) = previews.get(chat.id.as_str()) {
            chat.preview = Some((*preview).clone());
        }
    }
    chats.iter().filter(|chat| chat.preview.is_some()).count()
}

fn measure<F: Fn() -> usize>(run: F, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum += black_box(run());
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    for count in [100usize, 1000, 5000] {
        let chats = (0..count)
            .map(|index| Chat { id: format!("chat-{index}"), preview: None })
            .collect::<Vec<_>>();
        let previews = (0..count)
            .map(|index| (format!("chat-{index}"), Some(format!("preview-{index}"))))
            .collect::<Vec<_>>();
        let iterations = (10_000 / count).max(2);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..7 {
            before.push(measure(|| baseline(chats.clone(), &previews), iterations));
            after.push(measure(|| optimized(chats.clone(), &previews), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("chats={count} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
