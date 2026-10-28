use pain2note_lib::{db, importer, model::Filter, parser};
use std::io::Write;

const SAMPLE: &str = include_str!("../../fixtures/gg-six-max.txt");
fn hand(index: usize) -> &'static str {
    SAMPLE.split("\n\n").nth(index).unwrap()
}
fn connection() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    db::migrate(&c).unwrap();
    c
}

const MULTI: &str = include_str!("../../fixtures/gg-multi-run.txt");

fn import_paths(
    conn: &mut rusqlite::Connection,
    paths: &[String],
    hero: &str,
    progress: impl Fn(&pain2note_lib::model::ImportReport),
) -> Result<pain2note_lib::model::ImportReport, String> {
    importer::import_paths_with_workers(conn, paths, hero, progress, 2)
}

#[test]
fn cash_drop_reconciles_platform_money_and_keeps_hero_rake_separate() {
    let conn = connection();
    let base = parser::parse(hand(1), "Hero", &conn).unwrap();
    let raw = hand(1)
        .replace(
            "SmallBlind: posts",
            "Cash Drop to Pot : total $1\nSmallBlind: posts",
        )
        .replace("$6.50", "$7.50")
        .replace("$6.20", "$7.20");
    let parsed = parser::parse(&raw, "Hero", &conn).unwrap();
    assert_eq!(parsed.cash_drop, 100);
    assert_eq!(parsed.net, 420);
    assert_eq!(parsed.ev, Some(420.0));
    assert_eq!(parsed.hero_rake, base.hero_rake);
    assert_eq!(parsed.rake, base.rake);
    assert_eq!(parsed.players.iter().map(|p| p.invested).sum::<i64>(), 650);
    db::insert(&conn, &parsed).unwrap();
    assert_eq!(db::detail(&conn, &parsed.id).unwrap().cash_drop, 100);
    for broken in [
        raw.replace("total $1", "total $0"),
        raw.replace("total $1", "total $2"),
        raw.replace("total $1", "unknown $1"),
        raw.replace(
            "Cash Drop to Pot : total $1",
            "Cash Drop to Pot : total $1\nCash Drop to Pot : total $1",
        ),
        raw.replace("Cash Drop to Pot : total $1\n", "").replace(
            "*** HOLE CARDS ***",
            "*** HOLE CARDS ***\nCash Drop to Pot : total $1",
        ),
    ] {
        assert!(parser::parse(&broken, "Hero", &conn).is_err());
    }
    let folded = hand(2)
        .replace("Hero: posts", "Cash Drop to Pot : total $1\nHero: posts")
        .replace("$2.50", "$3.50");
    let parsed = parser::parse(&folded, "Hero", &conn).unwrap();
    assert_eq!(parsed.net, -50);
    assert_eq!(parsed.ev, Some(-50.0));
    let partial_reveal =
        folded.to_owned() + "\nSeat 3: PlayerThree showed [Ah] and collected ($3.50)";
    assert_eq!(
        parser::parse(&partial_reveal, "Hero", &conn).unwrap().net,
        -50
    );
    let unknown = MULTI.replace("showed [Qs Qh]", "showed [Qs]");
    assert!(parser::parse(&unknown, "Hero", &conn).unwrap().ev.is_none());
}

#[test]
fn cash_drop_increases_preflop_and_multi_runout_main_pot_ev() {
    let conn = connection();
    let base = parser::parse(hand(0), "Hero", &conn).unwrap();
    let raw = hand(0)
        .replace(
            "SmallBlind: posts",
            "Cash Drop to Pot : total $1\nSmallBlind: posts",
        )
        .replace("$200.50", "$201.50")
        .replace("$197.50", "$198.50");
    let parsed = parser::parse(&raw, "Hero", &conn).unwrap();
    let expected = (base.ev.unwrap() + 10000.0) * 19850.0 / 19750.0 - 10000.0;
    assert!((parsed.ev.unwrap() - expected).abs() < 1e-7);
    assert_eq!(parsed.net, base.net);
    assert_eq!(parsed.hero_rake, base.hero_rake);

    let base = parser::parse(MULTI, "Hero", &conn).unwrap();
    let raw = MULTI
        .replace(
            "Villain: posts",
            "Cash Drop to Pot : total $1\nVillain: posts",
        )
        .replace("Total pot $25", "Total pot $26")
        .replace("$7.35", "$7.85")
        .replace("$12.25", "$12.75");
    let parsed = parser::parse(&raw, "Hero", &conn).unwrap();
    let expected = (base.ev.unwrap() + 500.0) * (1600.0 * 2550.0 / 2600.0)
        / (1500.0 * 2450.0 / 2500.0)
        - 500.0;
    assert!((parsed.ev.unwrap() - expected).abs() < 1e-7);
    assert_eq!(parsed.net, 285);
    assert_eq!(parsed.boards.len(), 2);
    assert_eq!(parsed.hero_rake, None);
}

#[test]
fn parallel_workers_share_equity_cache_and_retry_invalid_duplicate_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("parallel.txt");
    // Two threads encounter the same uncached equity key concurrently.
    let second = hand(0).replace("RCDEMO001", "PARALLEL_SECOND");
    let broken = hand(1).replace("Total pot $6.50", "Total pot $7.50");
    std::fs::write(
        &path,
        [hand(0), &second, &broken, hand(1), hand(1)].join("\n\n"),
    )
    .unwrap();
    let mut conn = connection();
    let report = import_paths(&mut conn, &[path.to_string_lossy().into()], "Hero", |_| {}).unwrap();
    assert_eq!(report.workers, 2);
    assert_eq!(report.imported, 3, "{:?}", report.issues);
    assert_eq!(report.duplicates, 1);
    assert_eq!(report.rejected, 1);
    let count: i64 = conn
        .query_row("SELECT count(*) FROM equity_cache", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        db::detail(&conn, "RCDEMO001").unwrap().ev,
        db::detail(&conn, "PARALLEL_SECOND").unwrap().ev
    );
    let history = db::history(&conn).unwrap();
    assert_eq!(history[0]["report"]["workers"], 2);
}

#[test]
fn database_failure_rolls_back_the_entire_batch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hands.txt");
    std::fs::write(&path, SAMPLE).unwrap();
    let paths = [path.to_string_lossy().into_owned()];
    let mut conn = connection();
    conn.execute_batch("CREATE TRIGGER fail_hand BEFORE INSERT ON hands WHEN NEW.id='HDDEMO003' BEGIN SELECT RAISE(ABORT, 'test write failure'); END;").unwrap();
    let result = import_paths(&mut conn, &paths, "Hero", |_| {});
    assert!(result.unwrap_err().contains("test write failure"));
    assert_eq!(db::overview(&conn, Filter::default()).unwrap().hands, 0);
    assert!(db::history(&conn).unwrap().is_empty());
    let count: i64 = conn
        .query_row("SELECT count(*) FROM equity_cache", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn multiple_runouts_keep_shared_cards_side_pots_and_one_all_in_ev() {
    let conn = connection();
    let multi = parser::parse(MULTI, "Hero", &conn).unwrap();
    assert_eq!(multi.boards.len(), 2);
    assert_eq!(multi.board, multi.boards[0]);
    assert_eq!(&multi.boards[0][..4], &multi.boards[1][..4]);
    assert_eq!(multi.net, 235);
    assert_eq!(multi.rake, 30);
    assert_eq!(multi.other_fees, 20);
    assert_eq!(multi.hero_rake, None);
    assert!(multi.showdown && multi.ev.is_some());
    let single = MULTI
        .replace("FIRST ", "")
        .replace("Hand was run two times", "")
        .replace("*** SECOND RIVER *** [2c 7d 9h 3s] [Qd]\n", "")
        .replace("*** SECOND SHOWDOWN ***\n", "");
    assert_eq!(parser::parse(&single, "Hero", &conn).unwrap().ev, multi.ev);
    let summary_only = MULTI
        .lines()
        .filter(|l| !l.contains(" collected "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parser::parse(&summary_only, "Hero", &conn).unwrap().net,
        multi.net
    );
    assert!(db::insert(&conn, &multi).unwrap());
    assert_eq!(db::detail(&conn, &multi.id).unwrap().boards, multi.boards);
}

#[test]
fn preflop_twice_and_flop_three_times_sum_payouts_without_multiplying_ev() {
    let conn = connection();
    let single = parser::parse(hand(0), "Hero", &conn).unwrap();
    let twice = hand(0).replace("*** FLOP", "*** FIRST FLOP")
        .replace("*** TURN", "*** FIRST TURN").replace("*** RIVER", "*** FIRST RIVER")
        .replace("*** SHOWDOWN ***", "*** SECOND FLOP *** [4c 5d 6h]\n*** SECOND TURN *** [4c 5d 6h] [8s]\n*** SECOND RIVER *** [4c 5d 6h 8s] [Th]\n*** FIRST SHOWDOWN ***")
        .replace("BigBlind collected $197.50 from pot", "BigBlind collected $98.75 from pot\n*** SECOND SHOWDOWN ***\nHero collected $98.75 from pot");
    let parsed = parser::parse(&twice, "Hero", &conn).unwrap();
    assert_eq!(parsed.boards.len(), 2);
    assert_eq!(parsed.net, -125);
    assert_eq!(parsed.ev, single.ev);
    let three = MULTI.replace("MULTITURN", "MULTIFLOP")
        .replace("Hand was run two times", "Hand was run three times")
        .replace("*** FIRST TURN *** [2c 7d 9h] [3s]\n", "")
        .replace("*** FIRST RIVER", "*** FIRST TURN *** [2c 7d 9h] [3s]\n*** FIRST RIVER")
        .replace("*** SECOND RIVER *** [2c 7d 9h 3s] [Qd]", "*** SECOND TURN *** [2c 7d 9h] [4s]\n*** SECOND RIVER *** [2c 7d 9h 4s] [Qd]\n*** THIRD TURN *** [2c 7d 9h] [5s]\n*** THIRD RIVER *** [2c 7d 9h 5s] [6d]")
        .replace("Hero collected $7.35 from main pot", "Hero collected $4.90 from main pot")
        .replace("Villain collected $4.90 from side pot", "Villain collected $3.27 from side pot")
        .replace("Third collected $12.25 from pot", "Third collected $8.16 from pot\n*** THIRD SHOWDOWN ***\nHero collected $4.90 from main pot\nVillain collected $3.27 from side pot")
        .replace("Seat 1: Hero (button) showed [As Ah] and won ($7.35)", "Seat 1: Hero (button) showed [As Ah] and won ($4.90), and won ($4.90)")
        .replace("Seat 2: Villain (small blind) showed [Ks Kh] and won ($4.90)", "Seat 2: Villain (small blind) showed [Ks Kh] and won ($3.27), and won ($3.27)")
        .replace("Seat 3: Third (big blind) showed [Qs Qh] and won ($12.25)", "Seat 3: Third (big blind) showed [Qs Qh] and won ($8.16)");
    let parsed = parser::parse(&three, "Hero", &conn).unwrap();
    assert_eq!(parsed.boards.len(), 3);
    assert_eq!(&parsed.boards[0][..3], &parsed.boards[2][..3]);
    assert_eq!(parsed.net, 480);
    assert!(parsed.ev.is_some());
    let summary_only = three
        .lines()
        .filter(|line| !line.contains(" collected "))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        parser::parse(&summary_only, "Hero", &conn).unwrap().net,
        480
    );
}

#[test]
fn malformed_multi_runouts_are_rejected_and_unknown_holes_keep_ev_missing() {
    let conn = connection();
    for broken in [
        MULTI.replace("[Qd]", "[Ac]"),
        MULTI.replace("[Qd]", "[As]"),
        MULTI.replace("SECOND RIVER", "THIRD RIVER"),
        MULTI.replace("*** SECOND RIVER *** [2c 7d 9h 3s] [Qd]", ""),
        MULTI.replace("[2c 7d 9h 3s] [Qd]", "[2c 7d 9h 4s] [Qd]"),
        MULTI.replace("*** SECOND RIVER", "Hero: bets $1\n*** SECOND RIVER"),
    ] {
        assert!(parser::parse(&broken, "Hero", &conn).is_err());
    }
    let unknown = MULTI.replace("showed [Qs Qh]", "mucked");
    assert!(parser::parse(&unknown, "Hero", &conn).unwrap().ev.is_none());
}

#[test]
fn rake_is_separate_from_other_fees_and_normalized_per_hand_blind() {
    let conn = connection();
    let folded = hand(1).replace(
        "Dealt to Hero [Kh Qh]",
        "Dealt to Hero [Kh Qh]\nDealt to PlayerOne [Ac Ad]",
    );
    let folded = parser::parse(&folded, "PlayerOne", &conn).unwrap();
    assert_eq!(folded.rake, 30, "table rake includes opponents' pots");
    assert_eq!(
        folded.hero_rake,
        Some(0),
        "Hero paid nothing when folding without investing"
    );
    let a = parser::parse(
        &MULTI.replace("Rake $0.30 | Jackpot $0.20", "Rake $0.50 | Jackpot $0"),
        "Hero",
        &conn,
    )
    .unwrap();
    assert_eq!(a.hero_rake, Some(15));
    db::insert(&conn, &a).unwrap();
    let mut b = a.clone();
    b.id = "ANOTHER_STAKE".into();
    b.big_blind = 50;
    db::insert(&conn, &b).unwrap();
    let overview = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(overview.rake, Some(30));
    assert_eq!(overview.other_fees, 0);
    assert_eq!(overview.rake_known, 2);
    assert_eq!(overview.rake_missing, 0);
    assert_eq!(overview.known_rake, 30);
    assert!((overview.rake_bb100.unwrap() - 22.5).abs() < 1e-9);
    let filtered = db::overview(
        &conn,
        Filter {
            big_blind: Some(50),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.rake, Some(15));
    assert_eq!(filtered.rake_bb100, Some(30.0));
    let empty = db::overview(
        &conn,
        Filter {
            search: Some("NONE".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(empty.rake, Some(0));
    assert_eq!(empty.rake_bb100, Some(0.0));
}

#[test]
fn version_one_migration_splits_fees_without_changing_results() {
    let conn = connection();
    let raw = hand(1).replace("Rake $0.30 | Jackpot $0", "Rake $0.20 | Jackpot $0.10");
    let parsed = parser::parse(&raw, "Hero", &conn).unwrap();
    db::insert(&conn, &parsed).unwrap();
    let mut legacy = serde_json::to_value(&parsed).unwrap();
    for field in ["boards", "otherFees", "heroRake"] {
        legacy.as_object_mut().unwrap().remove(field);
    }
    legacy["rake"] = 30.into();
    conn.execute("UPDATE hands SET rake=30,data=?1", [legacy.to_string()])
        .unwrap();
    conn.execute_batch("ALTER TABLE hands DROP COLUMN other_fees; ALTER TABLE hands DROP COLUMN hero_rake; PRAGMA user_version=1;").unwrap();
    db::migrate(&conn).unwrap();
    db::migrate(&conn).unwrap();
    let result = db::detail(&conn, &parsed.id).unwrap();
    assert_eq!(result.rake, 20);
    assert_eq!(result.other_fees, 10);
    assert_eq!(result.hero_rake, parsed.hero_rake);
    assert_eq!(result.boards, vec![parsed.board]);
    assert_eq!(result.net, parsed.net);
    assert_eq!(result.ev, parsed.ev);
    let overview = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(overview.rake, Some(20));
    assert_eq!(overview.other_fees, 10);
}

#[test]
fn hero_rake_counts_won_pot_deductions_and_never_contributions() {
    let conn = connection();
    let loser = parser::parse(hand(0), "Hero", &conn).unwrap();
    assert_eq!(loser.hero_rake, Some(0));
    let winner = parser::parse(hand(1), "Hero", &conn).unwrap();
    assert_eq!(winner.hero_rake, Some(30));
    assert_eq!(
        winner
            .players
            .iter()
            .find(|p| p.name == "Hero")
            .unwrap()
            .invested,
        300
    );
    db::insert(&conn, &loser).unwrap();
    db::insert(&conn, &winner).unwrap();
    let complete = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(complete.rake, Some(30));
    assert_eq!(complete.rake_bb100, Some(15.0));
    assert_eq!(complete.known_rake, 30);
    assert_eq!(complete.net, loser.net + winner.net);

    let ambiguous = parser::parse(MULTI, "Hero", &conn).unwrap();
    assert_eq!(
        ambiguous.hero_rake, None,
        "GG does not attribute Jackpot separately to each winner"
    );
    db::insert(&conn, &ambiguous).unwrap();
    let incomplete = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(incomplete.rake, None);
    assert_eq!(incomplete.rake_bb100, None);
    assert_eq!(incomplete.rake_known, 2);
    assert_eq!(incomplete.rake_missing, 1);
    assert_eq!(incomplete.known_rake, 30);
    let filtered = db::overview(
        &conn,
        Filter {
            search: Some(winner.id),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(filtered.rake, Some(30));
    assert_eq!(filtered.rake_bb100, Some(30.0));
}

const TIED: &str = "Poker Hand #TIEDRAKE: Hold'em No Limit ($0.50/$1) - 2026/10/20 20:10:00
Table 'SyntheticTie' 6-max Seat #1 is the button
Seat 1: Hero ($10 in chips)
Seat 2: Villain ($10 in chips)
Hero: posts small blind $0.50
Villain: posts big blind $1
*** HOLE CARDS ***
Dealt to Hero [As Ad]
Hero: raises $9 to $10 and is all-in
Villain: calls $9 and is all-in
*** FLOP *** [2c 3d 4h]
*** TURN *** [2c 3d 4h] [8s]
*** RIVER *** [2c 3d 4h 8s] [9c]
*** SHOWDOWN ***
Hero collected $9.85 from pot
Villain collected $9.85 from pot
*** SUMMARY ***
Total pot $20 | Rake $0.30 | Jackpot $0
Seat 1: Hero showed [As Ad] and won ($9.85)
Seat 2: Villain showed [Ac Ah] and won ($9.85)";

#[test]
fn split_pot_rake_is_exact_only_when_the_fee_and_odd_chips_are_determined() {
    let conn = connection();
    let tied = parser::parse(TIED, "Hero", &conn).unwrap();
    assert_eq!(tied.hero_rake, Some(15));
    assert_eq!(
        tied.net, -15,
        "rake can be paid while Hero loses money on a tie"
    );
    let jackpot = TIED.replace("Rake $0.30 | Jackpot $0", "Rake $0.20 | Jackpot $0.10");
    assert_eq!(
        parser::parse(&jackpot, "Hero", &conn).unwrap().hero_rake,
        None
    );
    let unknown = TIED.replace("showed [Ac Ah]", "mucked");
    assert_eq!(
        parser::parse(&unknown, "Hero", &conn).unwrap().hero_rake,
        None
    );
    let odd = TIED
        .replace(
            "Hero: posts small blind",
            "Seat 3: Folder ($10 in chips)\nFolder: posts ante $0.01\nHero: posts small blind",
        )
        .replace("*** FLOP", "Folder: folds\n*** FLOP")
        .replace("Hero collected $9.85", "Hero collected $9.86")
        .replace("Total pot $20", "Total pot $20.01");
    assert_eq!(parser::parse(&odd, "Hero", &conn).unwrap().hero_rake, None);
    let no_rake = jackpot.replace("Rake $0.20 | Jackpot $0.10", "Rake $0 | Jackpot $0.30");
    assert_eq!(
        parser::parse(&no_rake, "Hero", &conn).unwrap().hero_rake,
        Some(0)
    );

    let hero_all_fees = MULTI
        .replace("Rake $0.30 | Jackpot $0.20", "Rake $0.10 | Jackpot $0.05")
        .replace("$4.90", "$5")
        .replace("$12.25", "$12.50");
    assert_eq!(
        parser::parse(&hero_all_fees, "Hero", &conn)
            .unwrap()
            .hero_rake,
        Some(10)
    );
}

#[test]
fn version_two_migration_discards_estimates_and_preserves_multiple_boards() {
    let conn = connection();
    let winner = parser::parse(hand(1), "Hero", &conn).unwrap();
    let multi = parser::parse(MULTI, "Hero", &conn).unwrap();
    for hand in [&winner, &multi] {
        db::insert(&conn, hand).unwrap();
    }
    conn.execute_batch("ALTER TABLE hands DROP COLUMN hero_rake; ALTER TABLE hands ADD COLUMN hero_rake REAL NOT NULL DEFAULT 0; PRAGMA user_version=2;").unwrap();
    for hand in [&winner, &multi] {
        let mut legacy = serde_json::to_value(hand).unwrap();
        legacy["heroRake"] = 12.34.into();
        conn.execute(
            "UPDATE hands SET hero_rake=12.34,data=?1 WHERE id=?2",
            rusqlite::params![legacy.to_string(), hand.id],
        )
        .unwrap();
    }
    db::migrate(&conn).unwrap();
    db::migrate(&conn).unwrap();
    assert_eq!(db::detail(&conn, &winner.id).unwrap().hero_rake, Some(30));
    let migrated = db::detail(&conn, &multi.id).unwrap();
    assert_eq!(migrated.hero_rake, None);
    assert_eq!(migrated.boards, multi.boards);
    assert_eq!(migrated.net, multi.net);
    assert_eq!(migrated.ev, multi.ev);
    let stored: Option<i64> = conn
        .query_row(
            "SELECT hero_rake FROM hands WHERE id=?1",
            [&multi.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, None);
    assert_eq!(db::overview(&conn, Filter::default()).unwrap().rake, None);
}

#[test]
fn parse_money_actions_refunds_and_exact_ev() {
    let conn = connection();
    let a = parser::parse(hand(0), "Hero", &conn).unwrap();
    assert_eq!(a.net, -10000);
    assert_eq!(a.position, "UTG");
    assert!(a.vpip && a.pfr && a.showdown);
    assert!(a.ev.unwrap() > 6000.0 && a.ev.unwrap() < 6200.0);
    let b = parser::parse(hand(1), "Hero", &conn).unwrap();
    assert_eq!(b.net, 320);
    assert_eq!(b.ev, Some(320.0));
    assert_eq!(b.position, "BTN");
    let c = parser::parse(hand(2), "Hero", &conn).unwrap();
    assert_eq!(c.net, -50);
    assert!(!c.vpip && !c.pfr);
    assert_eq!(c.position, "SB");
    let d = parser::parse(hand(3), "Hero", &conn).unwrap();
    assert_eq!(d.net, 3850);
    assert!(d.ev.unwrap() < d.net as f64);
    assert!(d.ev_note.contains("精确枚举"));
    let e = parser::parse(hand(4), "Hero", &conn).unwrap();
    assert_eq!(e.net, 9750);
    assert_eq!(e.ev, a.ev);
    assert!(e.ev_note.contains("查表"));
    assert_eq!(parser::money("$1,203.5").unwrap(), 120350);
    assert!(parser::money("$0.005").is_err());
}

#[test]
fn rejects_unsupported_corrupt_and_incomplete_hands() {
    let conn = connection();
    for text in [
        hand(0).replace("6-max", "9-max"),
        hand(0).replace("$200.50", "$201.50"),
        hand(0).replace("*** SUMMARY ***", ""),
        hand(0).replace("[Kd Kc]", "[As Kc]"),
        hand(0).replace("*** FLOP ***", "*** FIRST FLOP ***"),
        hand(0).replace("$0.50/$1", "$0.50/€1"),
        hand(0).replace("$0.50/$1", "$-0.50/$1"),
    ] {
        assert!(parser::parse(&text, "Hero", &conn).is_err());
    }
    assert!(parser::parse(hand(0), "WrongName", &conn).is_err());
    let no_cards = hand(0).replace("BigBlind: shows [Kd Kc]\n", "").replace(
        "Seat 2: BigBlind (big blind) showed [Kd Kc] and won ($197.50)",
        "",
    );
    assert!(parser::parse(&no_cards, "Hero", &conn)
        .unwrap()
        .ev
        .is_none());
}

#[test]
fn gg_whitespace_variants_and_heads_up_positions() {
    let conn = connection();
    let text = "Poker Hand #HDHEADSUP: Hold'em No Limit  ($0.50/$1) - 2026/10/20 20:00:00
Table 'HeadsUpOnSixMax' 6-max Seat #2 is the button
Seat 2: Hero ($100 in chips)
Seat 5: Villain ($100 in chips)
Hero: posts small blind $0.50
Villain: posts big blind $1
*** HOLE CARDS ***
Dealt to Hero [As Kh]
Dealt to Villain
Hero: folds
Uncalled bet ($0.50) returned to Villain
Villain collected $1 from pot
*** SUMMARY ***
Total pot $1 | Rake $0 | Jackpot $0 | Bingo $0
Seat 2: Hero (button) folded before Flop";
    let parsed = parser::parse(text, "Hero", &conn).unwrap();
    assert_eq!(parsed.net, -50);
    assert_eq!(parsed.ev, Some(-50.0));
    assert_eq!(parsed.position, "BTN");
    let big_blind = text.replace("Dealt to Villain\n", "Dealt to Villain [2c 3d]\n");
    assert_eq!(
        parser::parse(&big_blind, "Villain", &conn)
            .unwrap()
            .position,
        "BB"
    );
}

#[test]
fn recursive_folder_nested_zip_gzip_dedup_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input");
    std::fs::create_dir_all(input.join("nested")).unwrap();
    std::fs::write(input.join("hands.txt"), SAMPLE).unwrap();
    let zip_path = input.join("nested/again.zip");
    let mut archive = zip::ZipWriter::new(std::fs::File::create(zip_path).unwrap());
    archive
        .start_file("hands.txt", zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(SAMPLE.as_bytes()).unwrap();
    archive.finish().unwrap();
    let mut gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(input.join("copy.txt.gz")).unwrap(),
        flate2::Compression::default(),
    );
    gzip.write_all(SAMPLE.as_bytes()).unwrap();
    gzip.finish().unwrap();
    let db_path = dir.path().join("tracker.db");
    let mut conn = db::open(&db_path).unwrap();
    let report =
        import_paths(&mut conn, &[input.to_string_lossy().into()], "Hero", |_| {}).unwrap();
    assert_eq!(report.imported, 5, "{:?}", report.issues);
    assert_eq!(report.duplicates, 10);
    assert_eq!(report.rejected, 0);
    assert_eq!(report.workers, 2);
    drop(conn);
    let mut conn = db::open(&db_path).unwrap();
    let report =
        import_paths(&mut conn, &[input.to_string_lossy().into()], "Hero", |_| {}).unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(report.duplicates, 15);
    assert_eq!(report.workers, 0, "duplicates must not start workers");
    let result = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(result.hands, 5);
    assert_eq!(result.net, 3870);
    assert_eq!(result.ev_missing, 0);
    assert_eq!(result.points.last().unwrap().net, result.net);
    assert_eq!(db::list(&conn, Filter::default(), 0).unwrap().len(), 5);
    assert_eq!(db::history(&conn).unwrap().len(), 2);
    let cache_rows: i64 = conn
        .query_row("SELECT count(*) FROM equity_cache", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cache_rows, 1);
}

#[test]
fn filters_and_missing_ev_never_fabricate_a_curve() {
    let conn = connection();
    let mut a = parser::parse(hand(0), "Hero", &conn).unwrap();
    a.ev = None;
    db::insert(&conn, &a).unwrap();
    let b = parser::parse(hand(1), "Hero", &conn).unwrap();
    db::insert(&conn, &b).unwrap();
    let mut euro = b.clone();
    euro.id = "EUR".into();
    euro.currency = "EUR".into();
    db::insert(&conn, &euro).unwrap();
    let filter = Filter {
        currency: Some("USD".into()),
        ..Default::default()
    };
    let result = db::overview(&conn, filter).unwrap();
    assert_eq!(result.hands, 2);
    assert!(result.ev.is_none());
    assert_eq!(result.ev_missing, 1);
    assert!(result.points.iter().all(|p| p.ev.is_none()));
    let filter = Filter {
        currency: Some("USD".into()),
        search: Some("RCDEMO002".into()),
        ..Default::default()
    };
    assert_eq!(db::overview(&conn, filter).unwrap().hands, 1);
}

#[test]
fn tar_gzip_utf16_and_partial_errors_keep_valid_hands() {
    let dir = tempfile::tempdir().unwrap();
    let mut utf16 = vec![0xff, 0xfe];
    utf16.extend(SAMPLE.encode_utf16().flat_map(u16::to_le_bytes));
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(utf16.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "folder/hands.hh", utf16.as_slice())
        .unwrap();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tar.into_inner().unwrap()).unwrap();
    let archive = dir.path().join("hands.tgz");
    std::fs::write(&archive, gzip.finish().unwrap()).unwrap();
    let invalid = dir.path().join("broken.txt");
    std::fs::write(
        &invalid,
        hand(1)
            .replace("RCDEMO002", "BROKEN")
            .replace("$6.50", "$7.50"),
    )
    .unwrap();
    let mut conn = connection();
    let report = import_paths(
        &mut conn,
        &[
            archive.to_string_lossy().into(),
            invalid.to_string_lossy().into(),
        ],
        "Hero",
        |_| {},
    )
    .unwrap();
    assert_eq!(report.imported, 5, "{:?}", report.issues);
    assert_eq!(report.rejected, 1);
    assert!(report.issues[0].contains("底池无法对账"));
    assert_eq!(db::overview(&conn, Filter::default()).unwrap().net, 3870);
    assert_eq!(db::history(&conn).unwrap().len(), 1);
}

#[test]
fn large_missing_ev_series_is_sampled_and_keeps_the_boundary() {
    let mut conn = connection();
    let mut template = parser::parse(hand(1), "Hero", &conn).unwrap();
    let tx = conn.transaction().unwrap();
    for i in 0..5000 {
        template.id = format!("SAMPLE{i:05}");
        template.ev = if i < 6 { Some(320.0) } else { None };
        db::insert(&tx, &template).unwrap();
    }
    tx.commit().unwrap();
    let overview = db::overview(&conn, Filter::default()).unwrap();
    assert_eq!(overview.hands, 5000);
    assert_eq!(overview.ev_missing, 4994);
    assert!(overview.points.len() <= 1603);
    assert!(overview
        .points
        .iter()
        .any(|p| p.index == 6 && p.ev == Some(1920.0)));
    assert!(overview
        .points
        .iter()
        .any(|p| p.index == 7 && p.ev.is_none()));
    assert_eq!(overview.points.last().unwrap().index, 5000);
    assert_eq!(overview.points.last().unwrap().net, 1_600_000);
    assert_eq!(db::list(&conn, Filter::default(), 4950).unwrap().len(), 50);
    assert!(db::list(&conn, Filter::default(), 5000).unwrap().is_empty());
}
