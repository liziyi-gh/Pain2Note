use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub seat: u8,
    pub name: String,
    pub stack: i64,
    pub cards: Vec<String>,
    pub invested: i64,
    pub collected: i64,
    pub folded: bool,
    pub all_in: bool,
    pub street_bet: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hand {
    pub id: String,
    pub played_at: String,
    pub table_name: String,
    pub currency: String,
    pub small_blind: i64,
    pub big_blind: i64,
    pub hero: String,
    pub hero_cards: Vec<String>,
    pub position: String,
    pub players: Vec<Player>,
    pub board: Vec<String>,
    #[serde(default)]
    pub boards: Vec<Vec<String>>,
    pub net: i64,
    pub ev: Option<f64>,
    pub ev_note: String,
    pub rake: i64,
    #[serde(default)]
    pub other_fees: i64,
    #[serde(default)]
    pub cash_drop: i64,
    #[serde(default)]
    pub hero_rake: Option<i64>,
    pub vpip: bool,
    pub pfr: bool,
    pub showdown: bool,
    pub raw: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    #[serde(default)]
    pub workers: usize,
    pub files: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub issues: Vec<String>,
}

impl ImportReport {
    pub fn reject(&mut self, issue: String) {
        self.rejected += 1;
        if self.issues.len() < 100 {
            self.issues.push(issue);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub currency: Option<String>,
    pub big_blind: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub search: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub index: usize,
    pub played_at: String,
    pub net: i64,
    pub ev: Option<f64>,
    pub net_bb: f64,
    pub ev_bb: Option<f64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub hands: usize,
    pub net: i64,
    pub ev: Option<f64>,
    pub ev_known: usize,
    pub ev_missing: usize,
    pub bb100: f64,
    pub vpip: f64,
    pub pfr: f64,
    pub showdown: f64,
    pub rake: Option<i64>,
    pub other_fees: i64,
    pub rake_known: usize,
    pub rake_missing: usize,
    pub known_rake: i64,
    pub rake_bb100: Option<f64>,
    pub currency: String,
    pub currencies: Vec<String>,
    pub stakes: Vec<i64>,
    pub points: Vec<Point>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandRow {
    pub id: String,
    pub played_at: String,
    pub hero_cards: Vec<String>,
    pub position: String,
    pub big_blind: i64,
    pub net: i64,
    pub ev: Option<f64>,
    pub showdown: bool,
    pub currency: String,
}

impl From<Hand> for HandRow {
    fn from(h: Hand) -> Self {
        Self {
            id: h.id,
            played_at: h.played_at,
            hero_cards: h.hero_cards,
            position: h.position,
            big_blind: h.big_blind,
            net: h.net,
            ev: h.ev,
            showdown: h.showdown,
            currency: h.currency,
        }
    }
}
