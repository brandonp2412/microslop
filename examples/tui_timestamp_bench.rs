use std::hint::black_box;
use std::time::Instant;

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, Timelike, Weekday};

fn parse(raw: &str) -> Option<(NaiveDateTime, bool)> {
    let trimmed = raw.trim();
    let is_utc = trimmed.ends_with('Z');
    let stripped = trimmed.trim_end_matches('Z');
    let parsed = NaiveDateTime::parse_from_str(stripped, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(stripped, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    Some((parsed, is_utc))
}

fn format_parsed(parsed: Option<(NaiveDateTime, bool)>, raw: &str, today: NaiveDate) -> String {
    let Some((naive, is_utc)) = parsed else {
        return raw.to_owned();
    };
    let local_dt = if is_utc {
        naive.and_utc().with_timezone(&Local).naive_local()
    } else {
        naive
    };
    let ts_date = local_dt.date();
    if ts_date == today {
        format!("{:02}:{:02}", local_dt.hour(), local_dt.minute())
    } else {
        let days_ago = (today - ts_date).num_days();
        if days_ago > 0 && days_ago < 7 {
            let weekday = match ts_date.weekday() {
                Weekday::Mon => "Mon",
                Weekday::Tue => "Tue",
                Weekday::Wed => "Wed",
                Weekday::Thu => "Thu",
                Weekday::Fri => "Fri",
                Weekday::Sat => "Sat",
                Weekday::Sun => "Sun",
            };
            format!(
                "{} {:02}:{:02}",
                weekday,
                local_dt.hour(),
                local_dt.minute()
            )
        } else {
            let month = match ts_date.month() {
                1 => "Jan",
                2 => "Feb",
                3 => "Mar",
                4 => "Apr",
                5 => "May",
                6 => "Jun",
                7 => "Jul",
                8 => "Aug",
                9 => "Sep",
                10 => "Oct",
                11 => "Nov",
                12 => "Dec",
                _ => "???",
            };
            format!("{} {}", month, ts_date.day())
        }
    }
}

fn baseline(raw: &str, today: NaiveDate) -> String {
    format_parsed(parse(raw), raw, today)
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
    let today = Local::now().naive_local().date();
    let raw = "2026-09-08T12:34:56.123Z";
    let parsed = parse(raw);
    for messages in [20usize, 100, 500] {
        let iterations = 50_000 / messages;
        let mut before = Vec::new();
        let mut after = Vec::new();
        for _ in 0..9 {
            before.push(measure(
                || (0..messages).map(|_| baseline(raw, today).len()).sum(),
                iterations,
            ));
            after.push(measure(
                || {
                    (0..messages)
                        .map(|_| format_parsed(parsed, raw, today).len())
                        .sum()
                },
                iterations,
            ));
        }
        let before = median(before);
        let after = median(after);
        let gain = (before as f64 - after as f64) / before as f64 * 100.0;
        println!("messages={messages} baseline_us={before} optimized_us={after} gain={gain:.1}%");
    }
}
