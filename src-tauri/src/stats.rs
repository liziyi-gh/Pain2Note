//! Opportunity-based Hero statistics. Replay validated actions without recalculating EV.
use crate::model::Hand;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Frequency {
    pub hits: u64,
    pub opportunities: u64,
}

impl Frequency {
    fn record(&mut self, hit: bool) {
        self.opportunities += 1;
        self.hits += u64::from(hit);
    }

    fn merge(&mut self, other: &Self) {
        self.hits += other.hits;
        self.opportunities += other.opportunities;
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Aggression {
    pub bets: u64,
    pub raises: u64,
    pub calls: u64,
    pub folds: u64,
}

impl Aggression {
    fn merge(&mut self, other: &Self) {
        self.bets += other.bets;
        self.raises += other.raises;
        self.calls += other.calls;
        self.folds += other.folds;
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreetStats {
    pub seen: u64,
    pub cbet: Frequency,
    pub fold_to_cbet: Frequency,
    pub aggression: Aggression,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HudStats {
    pub vpip: Frequency,
    pub pfr: Frequency,
    pub three_bet: Frequency,
    pub four_bet: Frequency,
    pub fold_to_three_bet: Frequency,
    pub fold_to_four_bet: Frequency,
    pub wtsd: Frequency,
    pub won_at_showdown: Frequency,
    pub won_when_saw_flop: Frequency,
    pub aggression: Aggression,
    pub streets: [StreetStats; 3],
}

impl HudStats {
    pub fn merge(&mut self, other: &Self) {
        self.vpip.merge(&other.vpip);
        self.pfr.merge(&other.pfr);
        self.three_bet.merge(&other.three_bet);
        self.four_bet.merge(&other.four_bet);
        self.fold_to_three_bet.merge(&other.fold_to_three_bet);
        self.fold_to_four_bet.merge(&other.fold_to_four_bet);
        self.wtsd.merge(&other.wtsd);
        self.won_at_showdown.merge(&other.won_at_showdown);
        self.won_when_saw_flop.merge(&other.won_when_saw_flop);
        self.aggression.merge(&other.aggression);
        for (target, source) in self.streets.iter_mut().zip(&other.streets) {
            target.seen += source.seen;
            target.cbet.merge(&source.cbet);
            target.fold_to_cbet.merge(&source.fold_to_cbet);
            target.aggression.merge(&source.aggression);
        }
    }
}

struct Player<'a> {
    name: &'a str,
    remaining: i64,
    wager: i64,
    folded: bool,
    // Wager when this player last acted; None means action has not been taken.
    acted_at: Option<i64>,
}

static AMOUNT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[$€£¥]?[0-9][0-9,]*(?:\.[0-9]+)?").unwrap());

fn amount(action: &str) -> i64 {
    AMOUNT
        .find(action)
        .and_then(|m| crate::parser::money(m.as_str()).ok())
        .unwrap_or(0)
}

pub fn for_hand(hand: &Hand) -> HudStats {
    let mut stats = HudStats::default();
    stats.vpip.record(hand.vpip);
    stats.pfr.record(hand.pfr);
    let mut players: Vec<_> = hand
        .players
        .iter()
        .map(|p| Player {
            name: p.name.as_str(),
            remaining: p.stack,
            wager: 0,
            folded: false,
            acted_at: None,
        })
        .collect();
    let Some(hero) = players.iter().position(|p| p.name == hand.hero) else {
        return stats;
    };
    let mut street = 0;
    let mut raise_level = 0;
    let mut hero_raise_level = 0;
    let mut current_bet = hand.big_blind;
    let mut min_raise = hand.big_blind;
    let mut last_aggressor = None;
    let mut cbet_owner = None;
    let mut cbet_made = false;
    let mut street_aggression = false;
    let mut live_cbet = false;
    let mut owner_acted = false;

    for line in hand.raw.lines().map(str::trim) {
        if line.starts_with("*** SUMMARY") || line.contains("SHOWDOWN ***") {
            break;
        }
        if line.starts_with("*** SECOND") || line.starts_with("*** THIRD") {
            // Runouts cannot contain new decisions, and never multiply samples.
            break;
        }
        let next_street = if line.starts_with("*** ") && line.contains("FLOP ***") {
            Some(1)
        } else if line.starts_with("*** ") && line.contains("TURN ***") {
            Some(2)
        } else if line.starts_with("*** ") && line.contains("RIVER ***") {
            Some(3)
        } else {
            None
        };
        if let Some(next) = next_street {
            cbet_owner = if street == 0 || (cbet_made && last_aggressor == cbet_owner) {
                last_aggressor
            } else {
                None
            };
            street = next;
            if !players[hero].folded {
                stats.streets[street - 1].seen = 1;
            }
            for p in &mut players {
                p.wager = 0;
                p.acted_at = None;
            }
            current_bet = 0;
            min_raise = hand.big_blind;
            street_aggression = false;
            cbet_made = false;
            live_cbet = false;
            owner_acted = false;
            last_aggressor = None;
            continue;
        }
        let Some((actor, action)) = players.iter().enumerate().find_map(|(i, p)| {
            line.strip_prefix(p.name)
                .and_then(|s| s.strip_prefix(": "))
                .map(|a| (i, a))
        }) else {
            continue;
        };
        let fold = action.starts_with("folds");
        let call = action.starts_with("calls ");
        let bet = action.starts_with("bets ");
        let raise = action.starts_with("raises ");
        let check = action.starts_with("checks");
        let post = action.starts_with("posts ");
        if !(fold || call || bet || raise || check || post) {
            continue;
        }
        let to = if raise {
            amount(action.split_once(" to ").map_or("", |(_, to)| to))
        } else {
            players[actor].wager + amount(action)
        };
        let contribution = if call || bet || raise || post {
            to - players[actor].wager
        } else {
            0
        };
        if !post && actor == hero && players[hero].remaining > 0 {
            if street == 0 {
                let facing = current_bet > players[hero].wager;
                let can_raise = facing
                    && players[hero].remaining + players[hero].wager > current_bet
                    && players[hero]
                        .acted_at
                        .is_none_or(|at| current_bet - at >= min_raise)
                    && players.iter().enumerate().any(|(i, p)| {
                        i != hero && !p.folded && p.remaining + p.wager > current_bet
                    });
                if can_raise && raise_level == 1 && stats.three_bet.opportunities == 0 {
                    stats.three_bet.record(raise);
                }
                if can_raise && raise_level == 2 && stats.four_bet.opportunities == 0 {
                    stats.four_bet.record(raise);
                }
                if facing
                    && raise_level == 2
                    && hero_raise_level == 1
                    && stats.fold_to_three_bet.opportunities == 0
                {
                    stats.fold_to_three_bet.record(fold);
                }
                if facing
                    && raise_level == 3
                    && hero_raise_level == 2
                    && stats.fold_to_four_bet.opportunities == 0
                {
                    stats.fold_to_four_bet.record(fold);
                }
            } else {
                let street_stats = &mut stats.streets[street - 1];
                let a = &mut street_stats.aggression;
                a.bets += u64::from(bet);
                a.raises += u64::from(raise);
                a.calls += u64::from(call);
                a.folds += u64::from(fold);
                if live_cbet && stats.streets[street - 1].fold_to_cbet.opportunities == 0 {
                    stats.streets[street - 1].fold_to_cbet.record(fold);
                }
            }
        }
        if street > 0 && Some(actor) == cbet_owner && !owner_acted && !post {
            let opportunity = !street_aggression
                && players[actor].remaining > 0
                && players
                    .iter()
                    .enumerate()
                    .any(|(i, p)| i != actor && !p.folded && p.remaining > 0);
            if opportunity {
                cbet_made = bet;
                live_cbet = bet;
                if actor == hero {
                    stats.streets[street - 1].cbet.record(bet);
                }
            }
            owner_acted = true;
        }
        if bet || raise {
            if raise {
                live_cbet = false;
            }
            let increment = to - current_bet;
            if increment >= min_raise {
                min_raise = increment;
            }
            current_bet = current_bet.max(to);
            last_aggressor = Some(actor);
            street_aggression = true;
            if street == 0 {
                raise_level += 1;
                if actor == hero {
                    hero_raise_level = raise_level;
                }
            }
        }
        players[actor].remaining -= contribution;
        if !action.contains("ante") {
            players[actor].wager += contribution;
        }
        players[actor].folded |= fold;
        if !post {
            players[actor].acted_at = Some(current_bet);
        }
    }
    if stats.streets[0].seen > 0 {
        stats.wtsd.record(hand.showdown);
        let won = hand.players[hero].collected > 0;
        stats.won_when_saw_flop.record(won);
        if hand.showdown {
            stats.won_at_showdown.record(won);
        }
    }
    for s in &stats.streets {
        stats.aggression.merge(&s.aggression);
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Player as HandPlayer, parser};

    fn scenario(actions: &str) -> Hand {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut h = parser::parse(
            include_str!("../../fixtures/gg-six-max.txt")
                .split("\n\n")
                .nth(1)
                .unwrap(),
            "Hero",
            &conn,
        )
        .unwrap();
        h.players = ["Hero", "Villain", "Third"]
            .into_iter()
            .enumerate()
            .map(|(i, name)| HandPlayer {
                seat: i as u8 + 1,
                name: name.into(),
                stack: 10000,
                cards: vec![],
                invested: 0,
                collected: 0,
                folded: false,
                all_in: false,
                street_bet: 0,
            })
            .collect();
        h.raw = format!("Third: posts small blind $0.50\nVillain: posts big blind $1\n*** HOLE CARDS ***\n{actions}");
        h.showdown = false;
        h
    }

    fn assert_rate(f: &Frequency, hits: u64, opportunities: u64) {
        assert_eq!((f.hits, f.opportunities), (hits, opportunities));
    }

    #[test]
    fn preflop_counts_squeezes_cold_four_bets_and_folds_once() {
        let h = scenario("Villain: raises $2 to $3\nThird: calls $2.50\nHero: raises $8 to $11\nVillain: raises $15 to $26\nHero: folds");
        let s = for_hand(&h);
        assert_rate(&s.three_bet, 1, 1);
        assert_rate(&s.fold_to_four_bet, 1, 1);
        assert_rate(&s.four_bet, 0, 0);
        let h =
            scenario("Third: raises $2 to $3\nVillain: raises $6 to $9\nHero: raises $15 to $24");
        let s = for_hand(&h);
        assert_rate(&s.three_bet, 0, 0);
        assert_rate(&s.four_bet, 1, 1);
        let h =
            scenario("Hero: raises $2 to $3\nVillain: raises $6 to $9\nHero: raises $15 to $24");
        let s = for_hand(&h);
        assert_rate(&s.four_bet, 1, 1);
        assert_rate(&s.fold_to_three_bet, 0, 1);
    }

    #[test]
    fn all_in_and_short_raises_do_not_create_impossible_raise_opportunities() {
        let mut h = scenario("Hero: raises $2 to $3\nThird: folds\nVillain: raises $97 to $100 and is all-in\nHero: calls $97 and is all-in");
        let s = for_hand(&h);
        assert_rate(&s.four_bet, 0, 0);
        assert_rate(&s.fold_to_three_bet, 0, 1);

        h.raw = "Hero: raises $2 to $3\nThird: folds\nVillain: raises $1 to $4 and is all-in\nHero: calls $1".into();
        h.players[1].stack = 400;
        assert_rate(&for_hand(&h).four_bet, 0, 0);

        h.raw =
            "Hero: raises $2 to $3\nVillain: raises $6 to $9\nHero: calls $6 and is all-in".into();
        h.players[0].stack = 900;
        assert_rate(&for_hand(&h).four_bet, 0, 0);
    }

    #[test]
    fn barrels_follow_continuation_chain_on_all_three_streets() {
        let h = scenario("Hero: raises $2 to $3\nThird: folds\nVillain: calls $2\n*** FLOP *** [2c 3d 7h]\nVillain: checks\nHero: bets $4\nVillain: calls $4\n*** TURN *** [2c 3d 7h] [9s]\nVillain: checks\nHero: bets $10\nVillain: calls $10\n*** RIVER *** [2c 3d 7h 9s] [Jc]\nVillain: checks\nHero: checks");
        let s = for_hand(&h);
        assert_rate(&s.streets[0].cbet, 1, 1);
        assert_rate(&s.streets[1].cbet, 1, 1);
        assert_rate(&s.streets[2].cbet, 0, 1);
        assert_eq!(s.aggression.bets, 2);
        assert_eq!(s.streets.map(|s| s.seen), [1, 1, 1]);
    }

    #[test]
    fn donks_checkthroughs_and_raises_break_cbet_chain() {
        let base = "Hero: raises $2 to $3\nVillain: calls $2\n*** FLOP *** [2c 3d 7h]\n";
        let h = scenario(&format!("{base}Villain: bets $4\nHero: folds"));
        let s = for_hand(&h);
        assert_rate(&s.streets[0].cbet, 0, 0);
        assert_rate(&s.streets[0].fold_to_cbet, 0, 0);

        for flop in [
            "Villain: checks\nHero: checks",
            "Villain: checks\nHero: bets $4\nVillain: raises $4 to $8\nHero: calls $4",
        ] {
            let h = scenario(&format!(
                "{base}{flop}\n*** TURN *** [2c 3d 7h] [9s]\nVillain: checks\nHero: bets $10"
            ));
            assert_rate(&for_hand(&h).streets[1].cbet, 0, 0);
        }
    }

    #[test]
    fn fold_to_cbet_includes_checks_and_cold_calls_but_not_intervening_raises() {
        let base = "Villain: raises $2 to $3\nHero: calls $3\nThird: calls $2.50\n*** FLOP *** [2c 3d 7h]\nHero: checks\nVillain: bets $4\n";
        for (action, expected) in [
            ("Third: calls $4\nHero: folds", (1, 1)),
            ("Third: folds\nHero: raises $4 to $8", (0, 1)),
            ("Third: raises $4 to $8\nHero: folds", (0, 0)),
        ] {
            let h = scenario(&format!("{base}{action}"));
            let s = for_hand(&h);
            assert_rate(&s.streets[0].fold_to_cbet, expected.0, expected.1);
        }
    }

    #[test]
    fn turn_and_river_folds_to_continuation_bets_use_their_own_opportunities() {
        let base = "Villain: raises $2 to $3\nHero: calls $3\nThird: folds\n*** FLOP *** [2c 3d 7h]\nVillain: bets $4\nHero: calls $4\n*** TURN *** [2c 3d 7h] [9s]\nVillain: bets $10\n";
        let h = scenario(&format!(
            "{base}Hero: folds\n*** RIVER *** [2c 3d 7h 9s] [Jc]"
        ));
        let s = for_hand(&h);
        assert_rate(&s.streets[0].fold_to_cbet, 0, 1);
        assert_rate(&s.streets[1].fold_to_cbet, 1, 1);
        assert_eq!(s.streets[2].seen, 0);
        let h = scenario(&format!("{base}Hero: calls $10\n*** RIVER *** [2c 3d 7h 9s] [Jc]\nVillain: bets $20\nHero: folds"));
        let s = for_hand(&h);
        assert_rate(&s.streets[2].fold_to_cbet, 1, 1);
        assert_eq!(s.aggression.calls, 2);
        assert_eq!(s.aggression.folds, 1);
    }

    #[test]
    fn multiple_runouts_and_split_payouts_count_one_showdown_and_no_cbet() {
        let mut h = scenario("Hero: raises $99 to $100 and is all-in\nThird: folds\nVillain: calls $99 and is all-in\n*** FIRST FLOP *** [2c 3d 7h]\n*** FIRST TURN *** [2c 3d 7h] [9s]\n*** FIRST RIVER *** [2c 3d 7h 9s] [Jc]\n*** SECOND FLOP *** [4c 5d 8h]\n*** SECOND TURN *** [4c 5d 8h] [Ts]\n*** SECOND RIVER *** [4c 5d 8h Ts] [Qc]");
        h.showdown = true;
        h.players[0].collected = 5000; // A split payout is a win even with negative net.
        h.net = -5000;
        let s = for_hand(&h);
        assert_rate(&s.wtsd, 1, 1);
        assert_rate(&s.won_at_showdown, 1, 1);
        assert_rate(&s.won_when_saw_flop, 1, 1);
        assert_eq!(s.streets.each_ref().map(|s| s.seen), [1, 1, 1]);
        for street in &s.streets {
            assert_rate(&street.cbet, 0, 0);
        }
        assert_eq!(
            s.aggression.bets + s.aggression.raises + s.aggression.calls,
            0
        );
    }
}
