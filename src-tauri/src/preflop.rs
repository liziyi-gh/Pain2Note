//! Exact, suit-aware persistent equity table. No simulation occurs on a cache hit.
use crate::equity::card;
use pokers::{exact_equity, get_card_mask, HandRange};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{atomic::AtomicBool, Arc};

// v2 uses weighted winner masks, including fractional multiway ties. Entries
// written by v1 must not be reused because its tie aggregation could truncate.
const ENGINE: &str = "pokers-0.10.0-exact-v2";

/// Hero is first; opponents and cards within each hand are unordered.
/// Minimize across all 24 GLOBAL suit permutations (never per-player suits).
pub fn key(hands: &[Vec<String>], dead: &[String]) -> Result<String, String> {
    if !(2..=6).contains(&hands.len()) || hands.iter().any(|h| h.len() != 2) {
        return Err("翻前查表需要 2–6 位玩家的具体底牌".into());
    }
    let mut used = [false; 52];
    let hands: Vec<Vec<u8>> = hands
        .iter()
        .map(|h| h.iter().map(|c| card(c)).collect())
        .collect::<Result<_, _>>()?;
    let dead: Vec<u8> = dead.iter().map(|c| card(c)).collect::<Result<_, _>>()?;
    for &c in hands.iter().flatten().chain(dead.iter()) {
        if used[c as usize] {
            return Err("查表输入包含重复扑克牌".into());
        }
        used[c as usize] = true;
    }
    if used.iter().filter(|&&v| v).count() > 47 {
        return Err("剩余牌不足五张".into());
    }
    let mut best: Option<String> = None;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    let p = [a, b, c, d];
                    if (0..4).any(|i| p[..i].contains(&p[i])) {
                        continue;
                    }
                    let transform = |cards: &[u8]| {
                        let mut cards: Vec<u8> = cards
                            .iter()
                            .map(|&v| p[(v / 13) as usize] * 13 + v % 13)
                            .collect();
                        cards.sort_unstable();
                        cards.iter().map(|c| format!("{c:02x}")).collect::<String>()
                    };
                    let hero = transform(&hands[0]);
                    let mut opponents: Vec<String> =
                        hands[1..].iter().map(|h| transform(h)).collect();
                    opponents.sort();
                    let value = format!(
                        "{ENGINE}:{hero}:{}:{}",
                        opponents.join("-"),
                        transform(&dead)
                    );
                    if best.as_ref().is_none_or(|b| value < *b) {
                        best = Some(value);
                    }
                }
            }
        }
    }
    best.ok_or("无法规范化花色".into())
}

pub fn lookup(
    conn: &Connection,
    hands: &[Vec<String>],
    dead: &[String],
) -> Result<(f64, bool), String> {
    let key = key(hands, dead)?;
    let cached: Option<f64> = conn
        .query_row(
            "SELECT equity FROM equity_cache WHERE key=?1",
            [&key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(equity) = cached {
        return Ok((equity, true));
    }
    let mut ranges = HandRange::from_strings(hands.iter().map(|h| h.join("")).collect());
    // Specific hands default to weight 100 in pokers. Use unit weights so the
    // weighted masks count physical boards rather than 100^players units.
    for range in &mut ranges {
        for hand in &mut range.hands {
            hand.2 = 1;
        }
    }
    let result = exact_equity(
        &ranges,
        0,
        get_card_mask(&dead.join("")),
        1,
        Arc::new(AtomicBool::new(false)),
        |_| {},
    )
    .map_err(|e| format!("精确枚举失败：{e:?}"))?;
    // eval_count counts compressed evaluations, not represented boards. The
    // winner masks preserve their multiplicities and avoid the library's
    // integer division when aggregating split pots.
    let boards: u64 = result.wins_by_mask.iter().sum();
    let remaining = 52 - hands.len() as u64 * 2 - dead.len() as u64;
    let expected = (0..5).fold(1u64, |n, i| n * (remaining - i) / (i + 1));
    if boards != expected || result.wins_by_mask[0] != 0 {
        return Err(format!("权益枚举不完整：{boards} / {expected} 种公共牌"));
    }
    let share: f64 = result
        .wins_by_mask
        .iter()
        .enumerate()
        .filter(|(mask, _)| mask & 1 != 0)
        .map(|(mask, &count)| count as f64 / mask.count_ones() as f64)
        .sum();
    let equity = share / boards as f64;
    if !equity.is_finite() || !(0.0..=1.0).contains(&equity) || result.eval_count == 0 {
        return Err("权益引擎返回了无效结果".into());
    }
    conn.execute(
        "INSERT INTO equity_cache(key,equity,method,boards) VALUES (?1,?2,?3,?4) ON CONFLICT(key) DO NOTHING",
        params![key, equity, ENGINE, boards as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok((equity, false))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hands(a: &str, b: &str) -> Vec<Vec<String>> {
        vec![
            a.split_whitespace().map(str::to_string).collect(),
            b.split_whitespace().map(str::to_string).collect(),
        ]
    }
    #[test]
    fn keys_preserve_relative_suits_and_dead_cards() {
        assert_eq!(
            key(&hands("Ah Kh", "Qs Qd"), &[]).unwrap(),
            key(&hands("Kc Ac", "Qh Qs"), &[]).unwrap()
        );
        assert_ne!(
            key(&hands("Ah Kh", "Qh Qd"), &[]).unwrap(),
            key(&hands("Ah Kh", "Qs Qd"), &[]).unwrap()
        );
        assert_ne!(
            key(&hands("Ah Kh", "Qs Qd"), &["2c".into()]).unwrap(),
            key(&hands("Ah Kh", "Qs Qd"), &[]).unwrap()
        );
        assert!(key(&hands("Ah Ah", "Qs Qd"), &[]).is_err());
    }
    #[test]
    fn three_way_tie_keeps_fractional_shares_and_dead_cards() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let mut hands = hands("2c 3c", "2d 3d");
        hands.push(vec!["2h".into(), "3h".into()]);
        let allowed = [
            "2c", "3c", "2d", "3d", "2h", "3h", "As", "Ks", "Qs", "Js", "Ts",
        ];
        let dead: Vec<String> = b"cdhs"
            .iter()
            .flat_map(|&s| {
                b"23456789TJQKA"
                    .iter()
                    .map(move |&r| format!("{}{}", r as char, s as char))
            })
            .filter(|c| !allowed.contains(&c.as_str()))
            .collect();
        let (equity, hit) = lookup(&conn, &hands, &dead).unwrap();
        assert!(!hit);
        assert!((equity - 1.0 / 3.0).abs() < 1e-12);
        assert!(lookup(&conn, &hands, &dead).unwrap().1);
        let boards: i64 = conn
            .query_row("SELECT boards FROM equity_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(boards, 1);
    }
    #[test]
    fn exact_results_persist_across_connections_and_suit_permutations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("equity.db");
        let conn = crate::db::open(&path).unwrap();
        let (equity, hit) = lookup(&conn, &hands("As Ah", "Kd Kc"), &[]).unwrap();
        assert!(!hit);
        // Exhaustive AA vs KK reference: wins=1,388,072, ties=6,538 of C(48,5).
        let expected = (1_388_072.0 + 6_538.0 / 2.0) / 1_712_304.0;
        assert!((equity - expected).abs() < 1e-12, "{equity} != {expected}");
        drop(conn);
        let conn = crate::db::open(&path).unwrap();
        let (again, hit) = lookup(&conn, &hands("Ac Ad", "Kh Ks"), &[]).unwrap();
        assert!(hit);
        assert_eq!(again, equity);
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM equity_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let boards: i64 = conn
            .query_row("SELECT boards FROM equity_cache", [], |r| r.get(0))
            .unwrap();
        assert_eq!(boards, 1_712_304);
    }
}
