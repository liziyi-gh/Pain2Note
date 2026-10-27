use crate::{equity, model::Player};

/// Rake deducted from Hero's awarded pots, in cents. Never attribute rake by
/// investment or payout ratios. None means the exported totals are ambiguous.
pub fn hero_paid(
    players: &[Player],
    hero: &str,
    boards: &[Vec<String>],
    rake: i64,
    other_fees: i64,
    cash_drop: i64,
) -> Option<i64> {
    let hero = players.iter().position(|p| p.name == hero)?;
    if rake == 0 || players[hero].collected == 0 || players[hero].folded {
        return Some(0);
    }
    if players
        .iter()
        .enumerate()
        .all(|(i, p)| i == hero || p.collected == 0)
    {
        return Some(rake);
    }
    let mut levels: Vec<i64> = players
        .iter()
        .map(|p| p.invested)
        .filter(|&n| n > 0)
        .collect();
    levels.sort_unstable();
    levels.dedup();
    let mut previous = 0;
    let mut gross_min = 0;
    let mut gross_max = 0;
    for (pot, level) in levels.into_iter().enumerate() {
        let amount = (level - previous)
            * players.iter().filter(|p| p.invested >= level).count() as i64
            + if pot == 0 { cash_drop } else { 0 };
        previous = level;
        let eligible: Vec<usize> = players
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.folded && p.invested >= level)
            .map(|(i, _)| i)
            .collect();
        if !eligible.contains(&hero) {
            continue;
        }
        if eligible.len() == 1 {
            gross_min += amount;
            gross_max += amount;
            continue;
        }
        if boards.is_empty() {
            return None;
        }
        let runs = boards.len() as i64;
        for board in boards {
            if board.len() != 5 {
                return None;
            }
            let community = board
                .iter()
                .map(|c| equity::card(c).ok())
                .collect::<Option<Vec<_>>>()?;
            let mut scores = Vec::new();
            for &i in &eligible {
                if players[i].cards.len() != 2 {
                    return None;
                }
                let mut cards = players[i]
                    .cards
                    .iter()
                    .map(|c| equity::card(c).ok())
                    .collect::<Option<Vec<_>>>()?;
                cards.extend_from_slice(&community);
                scores.push((i, equity::rank(&cards)));
            }
            let best = scores.iter().map(|(_, rank)| rank).max()?;
            let winners: Vec<usize> = scores
                .iter()
                .filter(|(_, rank)| rank == best)
                .map(|(i, _)| *i)
                .collect();
            if !winners.contains(&hero) {
                continue;
            }
            let ties = winners.len() as i64;
            // Odd-chip assignment is not present in GG's fee totals. Keep a
            // range instead of choosing a rounding convention and calling it exact.
            gross_min += amount / runs / ties;
            gross_max += (amount + runs - 1) / runs / ties
                + i64::from(((amount + runs - 1) / runs) % ties != 0);
        }
    }
    let collected = players[hero].collected;
    let all_fees = rake + other_fees;
    let fees_min = (gross_min - collected).max(0);
    let fees_max = (gross_max - collected).min(all_fees);
    if fees_min > fees_max {
        return None;
    }
    // Only accept a unique cent value after accounting for the unspecified
    // distribution of Jackpot/Bingo/Fortune/Tax among the awarded pots.
    let rake_min = (fees_min - other_fees).max(0);
    let rake_max = fees_max.min(rake);
    (rake_min == rake_max).then_some(rake_min)
}
