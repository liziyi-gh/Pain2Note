// Evaluates 5–7 cards directly, with exact enumeration on flop/turn/river.
// Preflop uses an exact, persistent, suit-isomorphic SQLite lookup table.
use crate::model::Player;

pub fn card(text: &str) -> Result<u8, String> {
    let b = text.as_bytes();
    if b.len() != 2 {
        return Err(format!("无效扑克牌：{text}"));
    }
    let rank = b"23456789TJQKA".iter().position(|&r| r == b[0]);
    let suit = b"cdhs".iter().position(|&s| s == b[1]);
    match (rank, suit) {
        (Some(r), Some(s)) => Ok((s * 13 + r) as u8),
        _ => Err(format!("无效扑克牌：{text}")),
    }
}

fn straight(mask: u16) -> Option<u8> {
    for high in (4..13).rev() {
        if mask & (0b11111 << (high - 4)) == 0b11111 << (high - 4) {
            return Some(high as u8 + 2);
        }
    }
    if mask & 0b1_0000_0000_1111 == 0b1_0000_0000_1111 {
        Some(5)
    } else {
        None
    }
}

fn encode(category: u32, ranks: &[u8]) -> u32 {
    (0..5).fold(category, |v, i| {
        v * 15 + u32::from(*ranks.get(i).unwrap_or(&0))
    })
}

pub fn rank(cards: &[u8]) -> u32 {
    let mut counts = [0u8; 13];
    let mut suits = [0u16; 4];
    let mut mask = 0;
    for &c in cards {
        counts[(c % 13) as usize] += 1;
        suits[(c / 13) as usize] |= 1 << (c % 13);
        mask |= 1 << (c % 13);
    }
    let flush = suits.iter().find(|s| s.count_ones() >= 5).copied();
    if let Some(high) = flush.and_then(straight) {
        return encode(8, &[high]);
    }
    let ranks: Vec<u8> = (0..13)
        .rev()
        .filter(|&r| counts[r] > 0)
        .map(|r| r as u8 + 2)
        .collect();
    let groups = |n| -> Vec<u8> {
        ranks
            .iter()
            .copied()
            .filter(|&r| counts[(r - 2) as usize] >= n)
            .collect()
    };
    let quads = groups(4);
    if let Some(&q) = quads.first() {
        return encode(7, &[q, *ranks.iter().find(|&&r| r != q).unwrap_or(&0)]);
    }
    let trips = groups(3);
    let pairs = groups(2);
    if let Some(&t) = trips.first() {
        if let Some(&p) = pairs.iter().find(|&&p| p != t) {
            return encode(6, &[t, p]);
        }
    }
    if let Some(f) = flush {
        let top: Vec<u8> = (0..13)
            .rev()
            .filter(|&r| f & (1 << r) != 0)
            .take(5)
            .map(|r| r + 2)
            .collect();
        return encode(5, &top);
    }
    if let Some(high) = straight(mask) {
        return encode(4, &[high]);
    }
    if let Some(&t) = trips.first() {
        let mut top = vec![t];
        top.extend(ranks.iter().copied().filter(|&r| r != t).take(2));
        return encode(3, &top);
    }
    if pairs.len() >= 2 {
        return encode(
            2,
            &[
                pairs[0],
                pairs[1],
                *ranks.iter().find(|r| !pairs[..2].contains(r)).unwrap_or(&0),
            ],
        );
    }
    if let Some(&p) = pairs.first() {
        let mut top = vec![p];
        top.extend(ranks.iter().copied().filter(|&r| r != p).take(3));
        return encode(1, &top);
    }
    encode(0, &ranks[..ranks.len().min(5)])
}

pub fn adjusted_return(
    players: &[Player],
    hero: usize,
    board: &[String],
    rake: i64,
    conn: &rusqlite::Connection,
) -> Result<(f64, String), String> {
    adjusted_return_with_cash_drop(players, hero, board, rake, 0, conn)
}

pub fn adjusted_return_with_cash_drop(
    players: &[Player],
    hero: usize,
    board: &[String],
    rake: i64,
    cash_drop: i64,
    conn: &rusqlite::Connection,
) -> Result<(f64, String), String> {
    let mut used = [false; 52];
    let mut holes = Vec::new();
    for p in players {
        let mut hand = Vec::new();
        // Known folded cards are blockers, unknown folded cards stay in the deck.
        if !p.folded && p.cards.len() != 2 {
            return Err(format!("{} 未亮出底牌，无法计算全下权益", p.name));
        }
        for c in &p.cards {
            let c = card(c)?;
            if used[c as usize] {
                return Err("牌谱存在重复扑克牌".into());
            }
            used[c as usize] = true;
            hand.push(c);
        }
        holes.push(hand);
    }
    let mut runout = Vec::new();
    for c in board {
        let c = card(c)?;
        if used[c as usize] {
            return Err("牌谱存在重复扑克牌".into());
        }
        used[c as usize] = true;
        runout.push(c);
    }
    if runout.len() > 5 {
        return Err("不支持多次发牌".into());
    }
    let deck: Vec<u8> = (0..52).filter(|&c| !used[c as usize]).collect();
    let mut levels: Vec<i64> = players
        .iter()
        .map(|p| p.invested)
        .filter(|&x| x > 0)
        .collect();
    levels.sort_unstable();
    levels.dedup();
    let total: i64 = players.iter().map(|p| p.invested).sum::<i64>() + cash_drop;
    if total <= 0 || rake < 0 || rake > total || cash_drop < 0 {
        return Err("底池金额无效".into());
    }
    let mut pots = Vec::new();
    let mut previous = 0;
    for level in levels {
        // Platform money belongs to the main pot, including for a short stack.
        let amount = (level - previous)
            * players.iter().filter(|p| p.invested >= level).count() as i64
            + if previous == 0 { cash_drop } else { 0 };
        let eligible: Vec<usize> = players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.folded && p.invested >= level)
            .map(|(i, _)| i)
            .collect();
        if eligible.contains(&hero) {
            // Allocate recorded rake proportionally across pots; no estimated rake.
            pots.push((amount as f64 * (1.0 - rake as f64 / total as f64), eligible));
        }
        previous = level;
    }
    let payout = |full_board: &[u8]| -> f64 {
        let scores: Vec<u32> = players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if p.folded {
                    return 0;
                }
                let mut cards = holes[i].clone();
                cards.extend_from_slice(full_board);
                rank(&cards)
            })
            .collect();
        pots.iter()
            .map(|(amount, eligible)| {
                let best = eligible.iter().map(|&i| scores[i]).max().unwrap_or(0);
                if scores[hero] != best {
                    return 0.0;
                }
                amount / eligible.iter().filter(|&&i| scores[i] == best).count() as f64
            })
            .sum()
    };
    let missing = 5 - runout.len();
    if missing == 5 {
        let mut payout = 0.0;
        let mut all_cached = true;
        for (amount, eligible) in &pots {
            if eligible.len() == 1 {
                payout += amount;
                continue;
            }
            let mut hands = vec![players[hero].cards.clone()];
            hands.extend(
                eligible
                    .iter()
                    .filter(|&&i| i != hero)
                    .map(|&i| players[i].cards.clone()),
            );
            let dead: Vec<String> = players
                .iter()
                .enumerate()
                .filter(|(i, _)| !eligible.contains(i))
                .flat_map(|(_, p)| p.cards.clone())
                .collect();
            let (equity, hit) = crate::preflop::lookup(conn, &hands, &dead)?;
            all_cached &= hit;
            payout += amount * equity;
        }
        return Ok((
            payout - players[hero].invested as f64,
            if all_cached {
                "翻前 EV · SQLite 精确权益查表".into()
            } else {
                "翻前 EV · Rust 精确枚举并已存表".into()
            },
        ));
    }
    let mut sum = 0.0;
    let mut n = 0;
    if missing == 0 {
        sum = payout(&runout);
        n = 1;
    } else if missing == 1 {
        for &c in &deck {
            runout.push(c);
            sum += payout(&runout);
            n += 1;
            runout.pop();
        }
    } else if missing == 2 {
        for i in 0..deck.len() {
            for j in i + 1..deck.len() {
                runout.extend_from_slice(&[deck[i], deck[j]]);
                sum += payout(&runout);
                n += 1;
                runout.truncate(3);
            }
        }
    } else {
        return Err("公共牌数量必须为 0、3、4 或 5".into());
    }
    Ok((
        sum / n as f64 - players[hero].invested as f64,
        format!("全下 EV · 精确枚举 {n} 种发牌"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evaluator_agrees_with_independent_library_on_5000_comparisons() {
        let mut deck: Vec<u8> = (0..52).collect();
        let mut seed = 0x54b3_a918_d267_12f0u64;
        let external = |cards: &[u8]| {
            cards
                .iter()
                .fold(pokers::Hand::default(), |hand, &c| {
                    // Our suits: cdhs; pokers: shcd.
                    let suit = [2, 3, 1, 0][(c / 13) as usize];
                    hand + pokers::CARDS[(4 * (c % 13) + suit) as usize]
                })
                .evaluate()
        };
        for _ in 0..5000 {
            for i in 0..14 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let j = i + seed as usize % (52 - i);
                deck.swap(i, j);
            }
            assert_eq!(
                rank(&deck[..7]).cmp(&rank(&deck[7..14])),
                external(&deck[..7]).cmp(&external(&deck[7..14]))
            );
        }
    }

    #[test]
    fn main_and_side_pots_exclude_ineligible_players() {
        let player = |name: &str, amount: i64, cards: &[&str]| Player {
            seat: 1,
            name: name.into(),
            stack: amount,
            cards: cards.iter().map(|s| s.to_string()).collect(),
            invested: amount,
            collected: 0,
            folded: false,
            all_in: true,
            street_bet: amount,
        };
        let players = vec![
            player("Hero", 100, &["Ah", "Ad"]),
            player("Short", 50, &["Kh", "Kd"]),
            player("Deep", 100, &["Qh", "Qd"]),
        ];
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let board = ["Ks", "2c", "3d", "7h", "9c"].map(String::from);
        // Short wins 150 main pot; Hero wins 100 side pot; Hero's net is zero.
        assert_eq!(
            adjusted_return(&players, 0, &board, 0, &conn).unwrap().0,
            0.0
        );
        // 10% proportional fee removes 10 from Hero's side pot return.
        assert_eq!(
            adjusted_return(&players, 0, &board, 25, &conn).unwrap().0,
            -10.0
        );
        // Drop belongs to Short's main pot; Hero only wins the side pot.
        assert_eq!(
            adjusted_return_with_cash_drop(&players, 0, &board, 0, 50, &conn)
                .unwrap()
                .0,
            0.0
        );
        assert_eq!(
            adjusted_return_with_cash_drop(&players, 1, &board, 30, 50, &conn)
                .unwrap()
                .0,
            130.0
        );
    }
    fn score(s: &str) -> u32 {
        rank(
            &s.split_whitespace()
                .map(|c| card(c).unwrap())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn ranking_and_wheel() {
        assert!(score("As Ks Qs Js Ts 2d 3c") > score("Ah Ac Ad As Kd 3c 2d"));
        assert!(score("Ah Ac Ad Kc Kd 2c 3d") > score("As Js 8s 5s 3s Kd 2c"));
        assert!(score("6h 5s 4c 3d 2h Ks Qd") > score("As 2s 3d 4c 5h Kc Qc"));
        assert!(score("Ah Ad Ac Kh Kd Ks 2c") == score("Ah Ad Ac Kh Kd 3s 2c"));
        assert!(score("Ah Ad Kc Ks Qh 2d 3c") > score("Ah Ad Kc Ks Jh 2d 3c"));
    }
    #[test]
    fn exact_equity_splits_ties() {
        let p = |name: &str, cards: &[&str]| Player {
            seat: 1,
            name: name.into(),
            stack: 100,
            cards: cards.iter().map(|s| s.to_string()).collect(),
            invested: 100,
            collected: 0,
            folded: false,
            all_in: true,
            street_bet: 100,
        };
        let players = vec![p("Hero", &["2c", "3c"]), p("Villain", &["4d", "5d"])];
        let board = ["As", "Ks", "Qs", "Js", "Ts"].map(String::from);
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        assert_eq!(
            adjusted_return(&players, 0, &board, 10, &conn).unwrap().0,
            -5.0
        );
    }
}
