use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy)]
enum Item {
    TeamsHeader,
    Team(usize),
    Channel(usize, usize),
    ChatsHeader,
    Chat(usize),
}

fn flatten(teams: &[usize], chats: usize) -> Vec<Item> {
    let mut items = Vec::new();
    items.push(Item::TeamsHeader);
    for (team, &channels) in teams.iter().enumerate() {
        items.push(Item::Team(team));
        for channel in 0..channels {
            items.push(Item::Channel(team, channel));
        }
    }
    items.push(Item::ChatsHeader);
    for chat in 0..chats {
        items.push(Item::Chat(chat));
    }
    items
}

fn consume(items: &[Item]) -> usize {
    items
        .iter()
        .map(|item| match *item {
            Item::TeamsHeader | Item::ChatsHeader => 1,
            Item::Team(team) | Item::Chat(team) => team,
            Item::Channel(team, channel) => team + channel,
        })
        .sum()
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
    for chats in [50usize, 200, 1000] {
        let teams = vec![20; 10];
        let cached = flatten(&teams, chats);
        let iterations = 100_000 / chats;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| consume(&flatten(&teams, chats)), iterations));
            after.push(measure(|| consume(&cached), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("chats={chats} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
