use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PitchingStats {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub k_per_9: f64,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub games: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrikeoutLeader {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub strikeouts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtBatLeader {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub at_bats: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: String,
    pub name: String,
    pub ruthlessness: f64,
    pub patheticism: f64,
    #[serde(default)]
    pub pitching_rating: f64,
    #[serde(default)]
    pub hitting_rating: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub id: String,
    #[serde(with = "serde_with::rust::default_on_null")]
    pub team_id: String,
    pub data: Player,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub away_pitcher: Option<String>,
    pub away_pitcher_name: Option<String>,
    pub home_pitcher: Option<String>,
    pub home_pitcher_name: Option<String>,
    pub away_team: String,
    pub away_team_name: String,
    pub home_team: String,
    pub home_team_name: String,
    pub away_odds: f64,
    pub home_odds: f64,
    pub inning: isize,
    pub season: isize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Simulation {
    pub season: isize,
    pub day: usize,
    pub phase: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Games {
    pub sim: Simulation,
    pub schedule: Vec<Game>,
    pub tomorrow_schedule: Vec<Game>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventValue {
    pub games: Games,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub value: EventValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub full_name: String,
    pub lineup: Vec<String>,
    pub rotation: Vec<String>,
    pub bullpen: Vec<String>,
    pub bench: Vec<String>,
    pub perm_attr: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Idol {
    pub player_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameUpdates {
    pub next_page: String,
    pub data: Vec<GameUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameUpdate {
    pub data: Game,
}
