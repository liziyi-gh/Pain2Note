use crate::model::{Filter, Hand, HandRow, ImportReport, Overview, Point};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(30))
        .map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| e.to_string())?;
    migrate(&conn)?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> Result<(), String> {
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version > 1 {
        return Err("数据库由较新版本创建，请更新应用".into());
    }
    if version == 1 {
        return Ok(());
    }
    conn.execute_batch("BEGIN;
        CREATE TABLE IF NOT EXISTS hands (
          id TEXT PRIMARY KEY, played_at TEXT NOT NULL, currency TEXT NOT NULL,
          big_blind INTEGER NOT NULL CHECK(big_blind > 0), cards TEXT NOT NULL,
          net INTEGER NOT NULL, ev REAL, vpip INTEGER NOT NULL, pfr INTEGER NOT NULL,
          showdown INTEGER NOT NULL, rake INTEGER NOT NULL, data TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS hands_time ON hands(currency, played_at, id);
        CREATE INDEX IF NOT EXISTS hands_stake ON hands(currency, big_blind, played_at);
        CREATE TABLE IF NOT EXISTS imports (
          id INTEGER PRIMARY KEY, created_at TEXT NOT NULL DEFAULT (datetime('now')), report TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS equity_cache (
          key TEXT PRIMARY KEY, equity REAL NOT NULL CHECK(equity >= 0 AND equity <= 1),
          method TEXT NOT NULL, boards INTEGER NOT NULL, created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        PRAGMA user_version=1;
        COMMIT;").map_err(|e| e.to_string())
}

pub fn exists(conn: &Connection, id: &str) -> Result<bool, String> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM hands WHERE id=?1)",
        [id],
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

pub fn insert(conn: &Connection, hand: &Hand) -> Result<bool, String> {
    let json = serde_json::to_string(hand).map_err(|e| e.to_string())?;
    conn.execute("INSERT INTO hands (id,played_at,currency,big_blind,cards,net,ev,vpip,pfr,showdown,rake,data)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(id) DO NOTHING",
        params![hand.id, hand.played_at, hand.currency, hand.big_blind, hand.hero_cards.join(" "), hand.net,
            hand.ev, hand.vpip, hand.pfr, hand.showdown, hand.rake, json])
        .map(|n| n == 1).map_err(|e| e.to_string())
}

pub fn save_report(conn: &Connection, report: &ImportReport) -> Result<(), String> {
    conn.execute(
        "INSERT INTO imports(report) VALUES (?1)",
        [serde_json::to_string(report).map_err(|e| e.to_string())?],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn history(conn: &Connection) -> Result<Vec<serde_json::Value>, String> {
    let mut stmt = conn
        .prepare("SELECT created_at,report FROM imports ORDER BY id DESC LIMIT 30")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    rows.map(|r| { let (at, report) = r.map_err(|e| e.to_string())?; Ok(serde_json::json!({"at": at, "report": serde_json::from_str::<ImportReport>(&report).map_err(|e| e.to_string())?})) }).collect()
}

const WHERE: &str = "currency=?1 AND (?2 IS NULL OR big_blind=?2)
    AND (?3 IS NULL OR substr(played_at,1,10)>=?3) AND (?4 IS NULL OR substr(played_at,1,10)<=?4)
    AND instr(lower(id || ' ' || cards),lower(COALESCE(?5,'')))>0";

fn currencies(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare("SELECT DISTINCT currency FROM hands ORDER BY currency")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

fn currency(conn: &Connection, filter: &Filter) -> Result<String, String> {
    Ok(filter
        .currency
        .clone()
        .unwrap_or(currencies(conn)?.first().cloned().unwrap_or("USD".into())))
}

pub fn overview(conn: &Connection, filter: Filter) -> Result<Overview, String> {
    let currencies = currencies(conn)?;
    let currency = currency(conn, &filter)?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT big_blind FROM hands WHERE currency=?1 ORDER BY big_blind")
        .map_err(|e| e.to_string())?;
    let stakes = stmt
        .query_map([&currency], |r| r.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let bindings = params![
        currency,
        filter.big_blind,
        filter.from,
        filter.to,
        filter.search
    ];
    let hands: usize = conn
        .query_row(
            &format!("SELECT count(*) FROM hands WHERE {WHERE}"),
            bindings,
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let mut result = Overview {
        hands,
        net: 0,
        ev: Some(0.0),
        ev_known: 0,
        ev_missing: 0,
        bb100: 0.0,
        vpip: 0.0,
        pfr: 0.0,
        showdown: 0.0,
        rake: 0,
        currency,
        currencies,
        stakes,
        points: vec![],
    };
    let mut stmt = conn.prepare(&format!("SELECT played_at,net,ev,big_blind,vpip,pfr,showdown,rake FROM hands WHERE {WHERE} ORDER BY played_at,id")).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![
                result.currency,
                filter.big_blind,
                filter.from,
                filter.to,
                filter.search
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, f64>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, f64>(6)?,
                    r.get::<_, i64>(7)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;
    let mut net_bb = 0.0;
    let mut ev_bb = Some(0.0);
    let stride = hands.div_ceil(1600).max(1);
    let mut previous_point: Option<Point> = None;
    for (i, row) in rows.enumerate() {
        let (at, net, ev, bb, vpip, pfr, showdown, rake) = row.map_err(|e| e.to_string())?;
        let first_missing = result.ev.is_some() && ev.is_none();
        result.net += net;
        net_bb += net as f64 / bb as f64;
        result.ev = result.ev.zip(ev).map(|(a, b)| a + b);
        ev_bb = ev_bb.zip(ev).map(|(a, b)| a + b / bb as f64);
        if ev.is_some() {
            result.ev_known += 1;
        } else {
            result.ev_missing += 1;
        }
        result.vpip += vpip;
        result.pfr += pfr;
        result.showdown += showdown;
        result.rake += rake;
        // Keep the final known EV point and the first missing one. Subsequent
        // missing values must not bypass sampling for every hand in a large DB.
        if first_missing {
            if let Some(point) = previous_point.take() {
                if result.points.last().is_none_or(|p| p.index != point.index) {
                    result.points.push(point);
                }
            }
        }
        let point = Point {
            index: i + 1,
            played_at: at,
            net: result.net,
            ev: result.ev,
            net_bb,
            ev_bb,
        };
        if i % stride == 0 || i + 1 == hands || first_missing {
            result.points.push(point);
            previous_point = None;
        } else {
            previous_point = Some(point);
        }
    }
    if hands > 0 {
        let scale = 100.0 / hands as f64;
        result.bb100 = net_bb * scale;
        result.vpip *= scale;
        result.pfr *= scale;
        result.showdown *= scale;
    }
    Ok(result)
}

pub fn list(conn: &Connection, filter: Filter, offset: usize) -> Result<Vec<HandRow>, String> {
    let currency = currency(conn, &filter)?;
    let mut stmt = conn.prepare(&format!("SELECT data FROM hands WHERE {WHERE} ORDER BY played_at DESC,id DESC LIMIT 50 OFFSET ?6")).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(
            params![
                currency,
                filter.big_blind,
                filter.from,
                filter.to,
                filter.search,
                offset.min(i64::MAX as usize) as i64
            ],
            |r| r.get::<_, String>(0),
        )
        .map_err(|e| e.to_string())?;
    rows.map(|r| {
        let json = r.map_err(|e| e.to_string())?;
        serde_json::from_str::<Hand>(&json)
            .map(HandRow::from)
            .map_err(|e| e.to_string())
    })
    .collect()
}

pub fn detail(conn: &Connection, id: &str) -> Result<Hand, String> {
    let data: Option<String> = conn
        .query_row("SELECT data FROM hands WHERE id=?1", [id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&data.ok_or("手牌不存在")?).map_err(|e| e.to_string())
}
