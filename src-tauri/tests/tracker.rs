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
        importer::import_paths(&mut conn, &[input.to_string_lossy().into()], "Hero", |_| {})
            .unwrap();
    assert_eq!(report.imported, 5, "{:?}", report.issues);
    assert_eq!(report.duplicates, 10);
    assert_eq!(report.rejected, 0);
    drop(conn);
    let mut conn = db::open(&db_path).unwrap();
    let report =
        importer::import_paths(&mut conn, &[input.to_string_lossy().into()], "Hero", |_| {})
            .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(report.duplicates, 15);
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
    let report = importer::import_paths(
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
