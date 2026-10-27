//! Offline, resumable preflop table generation. Runtime imports use the same table.
//! cargo run --release --example precompute_equity -- /path/to/tracker.sqlite3 [--all]
use pain2note_lib::{db, preflop};
use std::{collections::BTreeSet, path::PathBuf, time::Instant};

fn card(c: u8) -> String {
    format!(
        "{}{}",
        b"23456789TJQKA"[(c % 13) as usize] as char,
        b"cdhs"[(c / 13) as usize] as char
    )
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Usage: precompute_equity DATABASE [--all]")?,
    );
    let all = args.next().is_some_and(|s| s == "--all");
    let conn = db::open(&path)?;
    let start = Instant::now();
    let mut count = 0;
    let mut misses = 0;
    if all {
        // Hero remains distinguished: reverse lookups are separately cached.
        let mut keys = BTreeSet::new();
        for a in 0..52 {
            for b in a + 1..52 {
                for c in 0..52 {
                    for d in c + 1..52 {
                        if [a, b].contains(&c) || [a, b].contains(&d) {
                            continue;
                        }
                        let hands = vec![vec![card(a), card(b)], vec![card(c), card(d)]];
                        let key = preflop::key(&hands, &[])?;
                        if !keys.insert(key) {
                            continue;
                        }
                        let (_, hit) = preflop::lookup(&conn, &hands, &[])?;
                        count += 1;
                        misses += usize::from(!hit);
                        if count % 500 == 0 {
                            println!(
                                "{count} unique matchups, {misses} newly enumerated, {:.1}s",
                                start.elapsed().as_secs_f64()
                            );
                        }
                    }
                }
            }
        }
    } else {
        for (a, b) in [
            ("As Ah", "Kd Kc"),
            ("As Ah", "Ks Kh"),
            ("As Ks", "Qd Qc"),
            ("As Kd", "Qh Qc"),
            ("As Ad", "Kh Qh"),
            ("9c Tc", "9d Th"),
            ("6s 6h", "5d 5c"),
            ("As Ah", "Ad Ac"),
        ] {
            let hands = vec![
                a.split_whitespace().map(str::to_string).collect(),
                b.split_whitespace().map(str::to_string).collect(),
            ];
            let (equity, hit) = preflop::lookup(&conn, &hands, &[])?;
            count += 1;
            misses += usize::from(!hit);
            println!(
                "{a} vs {b}: {:.8}% ({})",
                equity * 100.0,
                if hit { "table" } else { "exact -> saved" }
            );
        }
    }
    println!(
        "Done: {count} matchups; {misses} new; {:.2}s; {}",
        start.elapsed().as_secs_f64(),
        path.display()
    );
    Ok(())
}
