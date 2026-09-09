use std::hint::black_box;
use std::time::Instant;

fn baseline(member_ids: &[String]) -> usize {
    let raw_member_ids = member_ids.join(",");
    black_box(format!(
        "graph_raw id={} type={} topic={} hidden={} members={}",
        "19:chat", "oneOnOne", "Project discussion", false, raw_member_ids
    ))
    .len()
}

fn optimized(_: &[String]) -> usize {
    0
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
    for members in [2usize, 8, 32] {
        let member_ids = (0..members)
            .map(|index| format!("8:orgid:00000000-0000-0000-0000-{index:012}"))
            .collect::<Vec<_>>();
        let iterations = 100_000;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&member_ids), iterations));
            after.push(measure(|| optimized(&member_ids), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = if before == 0 { 0.0 } else { (before as f64 - after as f64) / before as f64 * 100.0 };
        println!("members={members} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
