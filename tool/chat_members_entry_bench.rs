use std::collections::{hash_map::Entry, HashMap};
use std::hint::black_box;
use std::time::{Duration, Instant};

#[derive(Clone)]
struct Members {
    names: Vec<String>,
    user_ids: Vec<String>,
}

fn baseline(map: &mut HashMap<String, Members>, names: Vec<String>, user_ids: Vec<String>) {
    map.entry("chat".to_owned())
        .and_modify(|members| {
            if members.names.is_empty() {
                members.names = names.clone();
            }
            if members.user_ids.is_empty() {
                members.user_ids = user_ids.clone();
            }
        })
        .or_insert(Members { names, user_ids });
}

fn optimized(map: &mut HashMap<String, Members>, names: Vec<String>, user_ids: Vec<String>) {
    match map.entry("chat".to_owned()) {
        Entry::Occupied(mut entry) => {
            let members = entry.get_mut();
            if members.names.is_empty() {
                members.names = names;
            }
            if members.user_ids.is_empty() {
                members.user_ids = user_ids;
            }
        }
        Entry::Vacant(entry) => {
            entry.insert(Members { names, user_ids });
        }
    }
}

fn measure(run: fn(&mut HashMap<String, Members>, Vec<String>, Vec<String>), iterations: usize) -> Duration {
    let mut map = HashMap::from([(
        "chat".to_owned(),
        Members {
            names: Vec::new(),
            user_ids: Vec::new(),
        },
    )]);
    let names = (0..4).map(|i| format!("Person {i}")).collect::<Vec<_>>();
    let user_ids = (0..4).map(|i| format!("8:orgid:{i:032x}")).collect::<Vec<_>>();
    let start = Instant::now();
    for _ in 0..iterations {
        map.get_mut("chat").unwrap().names.clear();
        map.get_mut("chat").unwrap().user_ids.clear();
        run(&mut map, black_box(names.clone()), black_box(user_ids.clone()));
    }
    black_box(map);
    start.elapsed()
}

fn percentile(values: &mut [Duration], numerator: usize, denominator: usize) -> Duration {
    values.sort_unstable();
    values[(values.len() - 1) * numerator / denominator]
}

fn main() {
    let mut before = (0..15).map(|_| measure(baseline, 500_000)).collect::<Vec<_>>();
    let mut after = (0..15).map(|_| measure(optimized, 500_000)).collect::<Vec<_>>();
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
