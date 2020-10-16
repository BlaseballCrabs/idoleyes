use anyhow::{anyhow, Result};
use chrono::prelude::*;
use idol_api::models::{AtBatLeader, Game, PitchingStats, Player, Position, StrikeoutLeader, Team};
use idol_api::State;
use idol_predictor::algorithms;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{read_dir, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let file = File::open(path)?;
    let buf = BufReader::new(file);
    let parsed = serde_json::from_reader(buf)?;
    Ok(parsed)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatsheet {
    pub player_id: String,
    pub team_id: String,
    pub strikeouts: usize,
    pub at_bats: usize,
    pub struckouts: usize,
}

#[derive(Debug, Default)]
pub struct PitchingData {
    pub strikeouts: usize,
    pub innings_pitched: usize,
}

#[derive(Debug, Default)]
pub struct StatState {
    pub pitchers: HashMap<String, PitchingData>,
    pub strikeouts: HashMap<String, StrikeoutLeader>,
    pub at_bats: HashMap<String, AtBatLeader>,
}

impl StatState {
    pub fn update(&mut self, base: &Path, day: usize, statsheet: PlayerStatsheet) -> Result<()> {
        if statsheet.strikeouts == 0 {
            self.strikeouts
                .entry(statsheet.player_id.clone())
                .or_insert_with(|| StrikeoutLeader {
                    player_id: statsheet.player_id.clone(),
                    strikeouts: 0,
                })
                .strikeouts += statsheet.struckouts;
            self.at_bats
                .entry(statsheet.player_id.clone())
                .or_insert_with(|| AtBatLeader {
                    player_id: statsheet.player_id.clone(),
                    at_bats: 0,
                })
                .at_bats += statsheet.at_bats;
        } else {
            let mut path = PathBuf::from(base);
            path.push("teams");
            path.push(&statsheet.team_id);
            path.push(&day.to_string());
            path.set_extension("json");
            let game: Game = read_json(path)?;
            let data = self
                .pitchers
                .entry(statsheet.player_id.clone())
                .or_default();
            data.strikeouts += statsheet.strikeouts;
            data.innings_pitched += game.inning + 1;
        }
        Ok(())
    }

    pub fn state(
        &self,
        base: &Path,
        day: usize,
        player_updates: &[PlayerUpdate],
        team_updates: &[TeamUpdate],
    ) -> Result<State> {
        let strikeouts = self.strikeouts.values().cloned().collect();
        let at_bats = self.at_bats.values().cloned().collect();
        let pitcher_stats = self
            .pitchers
            .iter()
            .map(|x| PitchingStats {
                player_id: x.0.clone(),
                player_name: x.0.clone(),
                k_per_9: (x.1.strikeouts * 9) as f64 / x.1.innings_pitched as f64,
            })
            .collect();
        let mut games = Vec::new();
        let mut path = PathBuf::from(base);
        path.push("games");
        path.push(&day.to_string());
        for entry in read_dir(path)? {
            let entry = entry?;
            let game: Game = read_json(entry.path())?;
            games.push(game);
        }
        let timestamp = Utc.ymd(2020, 10, 5).and_hms(16, 0, 0) + chrono::Duration::hours(day as _);
        let players = players_at(player_updates, timestamp);
        let teams = teams_at(team_updates, timestamp);
        Ok(State {
            strikeouts,
            at_bats,
            pitcher_stats,
            teams,
            players,
            games,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerUpdate {
    pub data: Player,
    pub first_seen: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerUpdatePage {
    pub next_page: String,
    pub data: Vec<serde_json::Value>,
}

pub fn get_player_updates() -> Result<Vec<serde_json::Value>> {
    let client = reqwest::blocking::Client::new();
    let mut player_updates = Vec::new();
    println!("getting player_updates");
    let mut page: PlayerUpdatePage = client
        .get("https://api.sibr.dev/chronicler/v1/players/updates?order=desc&count=1000")
        .send()?
        .json()?;
    loop {
        player_updates.extend(page.data);
        println!("page");
        let new: PlayerUpdatePage = client
            .get("https://api.sibr.dev/chronicler/v1/players/updates?order=desc&count=1000")
            .query(&[("page", &page.next_page)])
            .send()?
            .json()?;
        if new.next_page == page.next_page {
            break;
        } else {
            page = new;
        }
    }
    println!("done");
    Ok(player_updates)
}

pub fn players_at(player_updates: &[PlayerUpdate], time: DateTime<Utc>) -> Vec<Position> {
    player_updates
        .iter()
        .map(|x| &*x.data.id)
        .collect::<HashSet<&str>>()
        .into_iter()
        .filter_map(|x| {
            player_updates
                .iter()
                .skip_while(|y| y.first_seen > time)
                .find(|y| y.data.id == x)
                .map(|y| Position {
                    id: y.data.id.clone(),
                    team_id: "".to_string(), // TODO
                    data: y.data.clone(),
                })
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamUpdate {
    pub data: Team,
    pub first_seen: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamUpdatePage {
    pub next_page: Option<String>,
    pub data: Vec<serde_json::Value>,
}

pub fn get_team_updates() -> Result<Vec<serde_json::Value>> {
    let client = reqwest::blocking::Client::new();
    let mut team_updates = Vec::new();
    println!("getting team_updates");
    let mut page: TeamUpdatePage = client
        .get("https://api.sibr.dev/chronicler/v1/teams/updates?order=desc&count=250")
        .send()?
        .json()?;
    team_updates.extend(page.data);
    while let Some(next_page) = page.next_page.as_ref() {
        println!("page");
        let new: TeamUpdatePage = client
            .get("https://api.sibr.dev/chronicler/v1/teams/updates?order=desc&count=250")
            .query(&[("page", &page.next_page)])
            .send()?
            .json()?;
        if new
            .next_page
            .as_ref()
            .map(|x| x == next_page)
            .unwrap_or(true)
        {
            break;
        } else {
            page = new;
            team_updates.extend(page.data);
        }
    }
    println!("done");
    Ok(team_updates)
}

pub fn teams_at(team_updates: &[TeamUpdate], time: DateTime<Utc>) -> Vec<Team> {
    team_updates
        .iter()
        .map(|x| &*x.data.id)
        .collect::<HashSet<&str>>()
        .into_iter()
        .filter_map(|x| {
            team_updates
                .iter()
                .skip_while(|y| y.first_seen > time)
                .find(|y| y.data.id == x)
                .map(|x| x.data.clone())
        })
        .collect()
}

fn print_strikeouts(strat: &str, mut strikeouts: Vec<usize>) {
    println!("--- {} ---", strat);
    println!("strikeouts: {:?}", strikeouts);
    strikeouts.sort();
    println!(
        "mean: {}",
        strikeouts.iter().sum::<usize>() as f64 / strikeouts.len() as f64
    );
    println!("worst: {}", strikeouts[0]);
    println!("best: {}", strikeouts.last().unwrap());
    println!("median: {}", strikeouts[strikeouts.len() / 2]);
    println!("");
}

fn main() -> Result<()> {
    let base_raw = env::args().nth(1).ok_or_else(|| anyhow!("Base missing!"))?;
    let base = Path::new(&base_raw);
    let player_updates_raw = env::args().nth(2);
    let player_updates_path = player_updates_raw.as_ref().map(Path::new);
    let team_updates_raw = env::args().nth(3);
    let team_updates_path = team_updates_raw.as_ref().map(Path::new);
    let unchecked_player_updates = if let Some(path) = player_updates_path {
        read_json(path)?
    } else {
        let player_updates = get_player_updates()?;
        let file = File::create("player_updates.json")?;
        let buf = BufWriter::new(file);
        serde_json::to_writer(buf, &player_updates)?;
        player_updates
    };
    let player_updates: Vec<PlayerUpdate> = unchecked_player_updates
        .into_iter()
        .flat_map(serde_json::from_value)
        .collect();
    let unchecked_team_updates = if let Some(path) = team_updates_path {
        read_json(path)?
    } else {
        let team_updates = get_team_updates()?;
        let file = File::create("team_updates.json")?;
        let buf = BufWriter::new(file);
        serde_json::to_writer(buf, &team_updates)?;
        team_updates
    };
    let team_updates: Vec<TeamUpdate> = unchecked_team_updates
        .into_iter()
        .flat_map(serde_json::from_value)
        .collect();
    let mut state = StatState::default();
    let mut players = Vec::new();
    let mut players_path = PathBuf::from(base);
    players_path.push("players");

    for entry in read_dir(&players_path)? {
        let entry = entry?;
        players.push(entry.file_name());
    }

    let mut so9 = Vec::new();
    let mut ruthlessness = Vec::new();
    let mut stat_ratio = Vec::new();

    for day in 0..99 {
        let predictor = state.state(base, day, &player_updates, &team_updates)?;
        let best_so9 = algorithms::SO9.best_pitcher(&predictor).ok();
        let best_ruthlessness = algorithms::RUTHLESSNESS.best_pitcher(&predictor).ok();
        let best_stat_ratio = algorithms::STAT_RATIO.best_pitcher(&predictor).ok();

        for player in &players {
            let mut path = players_path.clone();
            path.push(player);
            path.push(&day.to_string());
            path.set_extension("json");
            let statsheet: PlayerStatsheet = match read_json(path) {
                Ok(x) => x,
                Err(_) => continue,
            };
            if day > 50 {
                if let Some(ref best) = &best_so9 {
                    if best.pitcher.player.id == statsheet.player_id {
                        so9.push(statsheet.strikeouts);
                    }
                }
                if let Some(ref best) = &best_ruthlessness {
                    if best.pitcher.player.id == statsheet.player_id {
                        ruthlessness.push(statsheet.strikeouts);
                    }
                }
                if let Some(ref best) = &best_stat_ratio {
                    if best.pitcher.player.id == statsheet.player_id {
                        stat_ratio.push(statsheet.strikeouts);
                    }
                }
            }
            state.update(base, day, statsheet)?;
        }
    }

    print_strikeouts("SO/9", so9);
    print_strikeouts("Ruthlessness", ruthlessness);
    print_strikeouts("(SO/9)(SO/AB)", stat_ratio);

    Ok(())
}
