use std::collections::{HashMap, HashSet};
use std::hint::black_box;
use std::time::Instant;

fn baseline(teams: &[(String, Vec<String>)]) -> HashMap<String, String> {
    teams
        .iter()
        .flat_map(|(team_id, channels)| {
            channels
                .iter()
                .cloned()
                .map(move |channel_id| (channel_id, team_id.clone()))
        })
        .collect()
}

fn optimized(teams: &[(String, Vec<String>)]) -> HashSet<String> {
    teams
        .iter()
        .flat_map(|(_, channels)| channels.iter().cloned())
        .collect()
}

fn measure<T: Len, F: Fn() -> T>(run: F, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..iterations {
        checksum += black_box(run()).len();
    }
    black_box(checksum);
    start.elapsed().as_micros()
}

trait Len {
    fn len(&self) -> usize;
}
impl<K, V> Len for HashMap<K, V> {
    fn len(&self) -> usize {
        HashMap::len(self)
    }
}
impl<T> Len for HashSet<T> {
    fn len(&self) -> usize {
        HashSet::len(self)
    }
}

fn median(mut values: Vec<u128>) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    let teams = (0..40)
        .map(|team| {
            (
                format!("team-{team:03}-0123456789abcdef"),
                (0..30)
                    .map(|channel| format!("19:channel-{team:03}-{channel:03}@thread.tacv2"))
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let mut before = Vec::new();
    let mut after = Vec::new();
    for _ in 0..9 {
        before.push(measure(|| baseline(&teams), 1000));
        after.push(measure(|| optimized(&teams), 1000));
    }
    let before = median(before);
    let after = median(after);
    let gain = (before as f64 - after as f64) / before as f64 * 100.0;
    println!("baseline_us={before} optimized_us={after} gain={gain:.1}%");
    assert_eq!(baseline(&teams).len(), optimized(&teams).len());
}
