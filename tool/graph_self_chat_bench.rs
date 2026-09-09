use std::hint::black_box;
use std::time::Instant;

fn ids_equal(left: &str, right: &str) -> bool {
    left.trim_matches(['{', '}'])
        .eq_ignore_ascii_case(right.trim_matches(['{', '}']))
}

fn baseline(member_ids: &[String], current_user_id: &str) -> bool {
    let member_ids = member_ids.iter().cloned().map(Some).collect::<Vec<_>>();
    !member_ids.is_empty()
        && member_ids.iter().all(|member_id| {
            member_id
                .as_deref()
                .is_some_and(|member_id| ids_equal(member_id, current_user_id))
        })
}

fn optimized(member_ids: &[String], current_user_id: &str) -> bool {
    let mut member_ids = member_ids.iter().map(|id| Some(id.as_str())).peekable();
    member_ids.peek().is_some()
        && member_ids.all(|member_id| {
            member_id.is_some_and(|member_id| ids_equal(member_id, current_user_id))
        })
}

fn measure<F: Fn() -> bool>(run: F, iterations: usize) -> u128 {
    let start = Instant::now();
    let mut checksum = 0;
    for _ in 0..iterations {
        checksum += black_box(run()) as usize;
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
        let current = "00000000-0000-0000-0000-000000000001";
        let member_ids = vec![current.to_owned(); members];
        assert_eq!(baseline(&member_ids, current), optimized(&member_ids, current));
        let iterations = 100_000;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(|| baseline(&member_ids, current), iterations));
            after.push(measure(|| optimized(&member_ids, current), iterations));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("members={members} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
