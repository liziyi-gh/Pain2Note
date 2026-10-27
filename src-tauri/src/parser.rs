use crate::{
    equity,
    model::{Hand, Player},
};
use regex::Regex;
use std::sync::LazyLock;

static HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Poker Hand #([A-Za-z0-9_-]+): Hold'em No Limit\s+\(([^/]+)/([^\)]+)\) - (\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2})").unwrap()
});
static TABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Table '(.+)' 6-max Seat #(\d) is the button").unwrap());
static SEAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Seat (\d): (.+) \(([^ ]+)(?: in chips)?\)").unwrap());
static AMOUNT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[$€£¥]?[0-9][0-9,]*(?:\.[0-9]+)?").unwrap());
static BRACKET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]").unwrap());
static REFUND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Uncalled bet \(([^)]+)\) returned to (.+)$").unwrap());
static COLLECT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+) collected ([$€£¥]?[0-9.,]+) from (?:main |side )?pot").unwrap()
});

pub fn money(s: &str) -> Result<i64, String> {
    let s = s
        .trim()
        .trim_start_matches(['$', '€', '£', '¥'])
        .replace(',', "");
    let (whole, frac) = s.split_once('.').unwrap_or((&s, ""));
    if frac.len() > 2
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !frac.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(format!("不支持的金额：{s}"));
    }
    let whole: i64 = whole.parse().map_err(|_| format!("无效金额：{s}"))?;
    let fraction: i64 = if frac.is_empty() {
        0
    } else {
        frac.parse::<i64>().map_err(|_| "无效小数")? * if frac.len() == 1 { 10 } else { 1 }
    };
    whole
        .checked_mul(100)
        .and_then(|n| n.checked_add(fraction))
        .filter(|&n| n <= 100_000_000_000)
        .ok_or("金额超出范围".into())
}

fn amount(s: &str) -> Result<i64, String> {
    money(
        AMOUNT
            .find(s)
            .ok_or_else(|| format!("缺少金额：{s}"))?
            .as_str(),
    )
}

fn cards(s: &str) -> Result<Vec<String>, String> {
    let result: Vec<String> = BRACKET
        .captures_iter(s)
        .flat_map(|c| {
            c[1].split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect();
    for c in &result {
        equity::card(c)?;
    }
    Ok(result)
}

fn lock_board(players: &[Player], board: &[String], locked: &mut Option<Vec<String>>) {
    let live: Vec<&Player> = players.iter().filter(|p| !p.folded).collect();
    if locked.is_none()
        && live.len() >= 2
        && live.iter().any(|p| p.all_in)
        && live.iter().filter(|p| !p.all_in).count() <= 1
    {
        *locked = Some(board.to_vec());
    }
}

pub fn parse(raw: &str, hero_name: &str, conn: &rusqlite::Connection) -> Result<Hand, String> {
    let raw = raw.trim().trim_start_matches('\u{feff}');
    if raw.len() > 256 * 1024 {
        return Err("单手牌谱超过 256 KiB".into());
    }
    if raw.contains("Tournament") {
        return Err("暂不支持锦标赛".into());
    }
    let h = HEADER
        .captures(raw.lines().next().unwrap_or(""))
        .ok_or("仅支持 GG Poker 英文 NLHE 现金桌牌谱")?;
    let id = h[1].to_string();
    let small_blind = money(&h[2])?;
    let big_blind = money(&h[3])?;
    if small_blind <= 0 || big_blind < small_blind {
        return Err("盲注金额无效".into());
    }
    let currency = if h[2].contains('$') {
        "USD"
    } else if h[2].contains('€') {
        "EUR"
    } else if h[2].contains('£') {
        "GBP"
    } else if h[2].contains('¥') {
        "CNY"
    } else {
        return Err("缺少支持的现金币种".into());
    };
    if h[2].trim().chars().next() != h[3].trim().chars().next() {
        return Err("大小盲币种不一致".into());
    }
    let played_at = chrono::NaiveDateTime::parse_from_str(&h[4], "%Y/%m/%d %H:%M:%S")
        .map_err(|_| "无效手牌时间")?
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let mut table_name = String::new();
    let mut button = 0;
    let mut players: Vec<Player> = Vec::new();
    let mut board = Vec::new();
    let mut locked = None;
    let mut first_all_in_street = None;
    let mut vpip = false;
    let mut pfr = false;
    let mut summary = false;
    let mut in_actions = false;
    let mut total_pot = None;
    let mut rake = 0;
    let special = [
        "FIRST FLOP",
        "SECOND FLOP",
        "FIRST TURN",
        "SECOND TURN",
        "FIRST RIVER",
        "SECOND RIVER",
        "Cashout",
        "cashout",
        "cashed out",
        "Insurance",
        "insurance",
        "Cash Drop",
        "cash drop",
    ]
    .iter()
    .any(|s| raw.contains(s));
    if special {
        return Err("暂不支持多次发牌、保险、现金退出或 Cash Drop 牌谱".into());
    }
    for line in raw.lines().map(str::trim) {
        if let Some(c) = TABLE.captures(line) {
            table_name = c[1].into();
            button = c[2].parse().unwrap();
            continue;
        }
        if !in_actions && !summary {
            if let Some(c) = SEAT.captures(line) {
                players.push(Player {
                    seat: c[1].parse().unwrap(),
                    name: c[2].into(),
                    stack: money(&c[3])?,
                    cards: vec![],
                    invested: 0,
                    collected: 0,
                    folded: false,
                    all_in: false,
                    street_bet: 0,
                });
                continue;
            }
        }
        if line.starts_with("*** HOLE CARDS") {
            in_actions = true;
            continue;
        }
        if line.starts_with("*** FLOP")
            || line.starts_with("*** TURN")
            || line.starts_with("*** RIVER")
        {
            lock_board(&players, &board, &mut locked);
            let next = cards(line)?;
            let expected = if line.starts_with("*** FLOP") {
                3
            } else if line.starts_with("*** TURN") {
                4
            } else {
                5
            };
            if next.len() != expected
                || !next.starts_with(&board)
                || (expected != 3 && board.len() != expected - 1)
                || (expected == 3 && !board.is_empty())
            {
                return Err("公共牌顺序或数量无效".into());
            }
            board = next;
            for p in &mut players {
                p.street_bet = 0;
            }
            continue;
        }
        if line.starts_with("*** SHOW") || line.starts_with("*** SUMMARY") {
            lock_board(&players, &board, &mut locked);
            if line.starts_with("*** SUMMARY") {
                summary = true;
            }
            continue;
        }
        if let Some(c) = REFUND.captures(line) {
            let p = players
                .iter_mut()
                .find(|p| p.name == c[2])
                .ok_or("退回筹码的玩家不存在")?;
            let refund = money(&c[1])?;
            if refund > p.invested || refund > p.street_bet {
                return Err("未跟注退回金额异常".into());
            }
            p.invested -= refund;
            p.street_bet -= refund;
            continue;
        }
        if let Some(c) = COLLECT.captures(line) {
            let p = players
                .iter_mut()
                .find(|p| p.name == c[1])
                .ok_or("获胜玩家不存在")?;
            p.collected += money(&c[2])?;
            continue;
        }
        if line.starts_with("Total pot ") {
            total_pot = Some(amount(line)?);
            // GG lists jackpot/bingo/fortune deductions separately from rake.
            for part in line.split('|').skip(1) {
                let part = part.trim();
                if ["Rake", "Jackpot", "Bingo", "Fortune", "Tax"]
                    .iter()
                    .any(|label| part.starts_with(label))
                {
                    rake += amount(part)?;
                } else if AMOUNT.find(part).is_some() && amount(part)? != 0 {
                    return Err(format!("未知底池扣费：{part}"));
                }
            }
            continue;
        }
        if line.starts_with("Dealt to ") {
            let name = line
                .trim_start_matches("Dealt to ")
                .split(" [")
                .next()
                .unwrap_or("");
            if let Some(p) = players.iter_mut().find(|p| p.name == name) {
                let c = cards(line)?;
                if !c.is_empty() {
                    p.cards = c;
                }
            }
            continue;
        }
        if summary {
            for p in &mut players {
                if line.starts_with(&format!("Seat {}: {} ", p.seat, p.name)) {
                    if line.contains(" showed [") || line.contains(" mucked [") {
                        p.cards = cards(line)?;
                    }
                    if p.collected == 0 {
                        if let Some((_, won)) = line
                            .split_once(" won (")
                            .or_else(|| line.split_once(" collected ("))
                        {
                            p.collected = amount(won)?;
                        }
                    }
                }
            }
            continue;
        }
        if let Some((index, action)) = players
            .iter()
            .enumerate()
            .find_map(|(i, p)| line.strip_prefix(&format!("{}: ", p.name)).map(|a| (i, a)))
        {
            let p = &mut players[index];
            if action.starts_with("folds") {
                p.folded = true;
                continue;
            }
            if action.starts_with("shows [") {
                p.cards = cards(action)?;
                continue;
            }
            if action.starts_with("checks")
                || action.starts_with("mucks")
                || action.starts_with("doesn't show")
            {
                continue;
            }
            let contribution = if action.starts_with("raises ") {
                let to = amount(action.split_once(" to ").ok_or("加注缺少 to 金额")?.1)?;
                to.checked_sub(p.street_bet)
                    .filter(|&n| n > 0)
                    .ok_or("加注金额无效")?
            } else if action.starts_with("calls ")
                || action.starts_with("bets ")
                || action.starts_with("posts ")
            {
                if action.contains("small & big") || action.contains("dead") {
                    return Err("暂不支持复合补盲或死盲格式".into());
                }
                amount(action)?
            } else if action.starts_with("sits out") || action.starts_with("is sitting out") {
                continue;
            } else {
                return Err(format!("未识别的玩家动作：{action}"));
            };
            if locked.is_some() && contribution > 0 {
                return Err("全下后仍有额外下注，无法可靠解析".into());
            }
            p.invested += contribution;
            if !action.contains("ante") {
                p.street_bet += contribution;
            }
            if p.invested > p.stack {
                return Err(format!("{} 的投入超过起始筹码", p.name));
            }
            if p.name == hero_name && board.is_empty() && !action.starts_with("posts ") {
                vpip = true;
                if action.starts_with("raises ") {
                    pfr = true;
                }
            }
            if action.contains("all-in") || p.invested == p.stack {
                p.all_in = true;
                if first_all_in_street.is_none() {
                    first_all_in_street = Some(board.len());
                }
            }
        }
    }
    if table_name.is_empty() {
        return Err("仅支持 6-max 牌桌".into());
    }
    if !summary || total_pot.is_none() {
        return Err("牌谱不完整：缺少 SUMMARY / Total pot".into());
    }
    if !(2..=6).contains(&players.len()) {
        return Err("玩家数量应为 2–6 人".into());
    }
    for (i, p) in players.iter().enumerate() {
        if !(1..=6).contains(&p.seat)
            || players[..i]
                .iter()
                .any(|q| q.seat == p.seat || q.name == p.name)
        {
            return Err("重复玩家或无效座位".into());
        }
    }
    let hero = players
        .iter()
        .position(|p| p.name == hero_name)
        .ok_or_else(|| format!("未找到 Hero 玩家“{hero_name}”，请检查导入设置"))?;
    if players[hero].cards.len() != 2 {
        return Err("缺少 Hero 的两张底牌".into());
    }
    let mut seen = [false; 52];
    for c in players
        .iter()
        .flat_map(|p| p.cards.iter())
        .chain(board.iter())
    {
        let card = equity::card(c)? as usize;
        if seen[card] {
            return Err("牌谱存在重复扑克牌".into());
        }
        seen[card] = true;
    }
    if players
        .iter()
        .any(|p| !p.cards.is_empty() && p.cards.len() != 2)
    {
        return Err("底牌数量无效".into());
    }
    let invested: i64 = players.iter().map(|p| p.invested).sum();
    let collected: i64 = players.iter().map(|p| p.collected).sum();
    if total_pot != Some(invested) || collected + rake != invested {
        return Err(format!(
            "底池无法对账：投入 {invested}，派彩 {collected}，扣费 {rake}（分）"
        ));
    }
    let hero_player = &players[hero];
    let net = hero_player.collected - hero_player.invested;
    let live = players.iter().filter(|p| !p.folded).count();
    let (ev, ev_note) = if hero_player.folded || live < 2 || first_all_in_street.is_none() {
        (Some(net as f64), "未参与全下摊牌，EV 等于实际盈利".into())
    } else if let Some(ref frozen) = locked {
        if Some(frozen.len()) != first_all_in_street {
            (None, "多轮下注的边池：暂不计算 EV".into())
        } else if frozen.len() == 5 {
            (Some(net as f64), "河牌全下，EV 等于实际盈利".into())
        } else {
            match equity::adjusted_return(&players, hero, frozen, rake, conn) {
                Ok((value, note)) => (Some(value), note),
                Err(reason) => (None, reason),
            }
        }
    } else {
        (None, "无法确定全下时点，EV 缺失".into())
    };
    let mut seats: Vec<u8> = players.iter().map(|p| p.seat).collect();
    seats.sort_unstable();
    let button_index = seats
        .iter()
        .position(|&s| s == button)
        .ok_or("按钮座位不存在")?;
    let hero_index = seats.iter().position(|&s| s == hero_player.seat).unwrap();
    let offset = (hero_index + seats.len() - button_index) % seats.len();
    let position = if offset == 0 {
        "BTN"
    } else if seats.len() == 2 {
        "BB"
    } else if offset == 1 {
        "SB"
    } else if offset == 2 {
        "BB"
    } else {
        ["UTG", "HJ", "CO"][6 - seats.len() + offset - 3]
    };
    Ok(Hand {
        id,
        played_at,
        table_name,
        currency: currency.into(),
        small_blind,
        big_blind,
        hero: hero_name.into(),
        hero_cards: hero_player.cards.clone(),
        position: position.into(),
        showdown: raw.contains("*** SHOW") && !hero_player.folded,
        players,
        board,
        net,
        ev,
        ev_note,
        rake,
        vpip,
        pfr,
        raw: raw.into(),
    })
}
