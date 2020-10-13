#![feature(array_value_iter)]

use anyhow::{anyhow, Result};
use chrono::prelude::*;
use eventsource::reqwest::Client;
use noisy_float::prelude::*;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::array::IntoIter;
use std::fmt::Write;

#[derive(Debug, Serialize, Deserialize)]
pub struct PitchingStats {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub k_per_9: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: String,
    pub name: String,
    pub ruthlessness: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    pub away_pitcher: String,
    pub away_pitcher_name: String,
    pub home_pitcher: String,
    pub home_pitcher_name: String,
    pub away_team: String,
    pub away_team_name: String,
    pub home_team: String,
    pub home_team_name: String,
    pub away_odds: f64,
    pub home_odds: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Simulation {
    pub season: usize,
    pub day: usize,
    pub phase: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Games {
    pub sim: Simulation,
    pub tomorrow_schedule: Vec<Game>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EventValue {
    pub games: Games,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub value: EventValue,
}

#[derive(Debug, Serialize)]
pub struct Webhook<'a> {
    pub content: &'a str,
}

fn get_best() -> Result<Option<String>> {
    let mut client =
        Client::new(Url::parse("https://www.blaseball.com/events/streamData").unwrap());
    let event = client
        .next()
        .ok_or_else(|| anyhow!("Didn't get event!"))??;
    let data: Event = serde_json::from_str(&event.data)?;
    if data.value.games.sim.phase != 2 {
        return Ok(None);
    }
    let comma_pitchers = data
        .value
        .games
        .tomorrow_schedule
        .iter()
        .flat_map(|x| IntoIter::new([&*x.home_pitcher, &*x.away_pitcher]))
        .collect::<Vec<&str>>()
        .join(",");
    let best_so9 =
        reqwest::blocking::get(&format!(
            "https://api.blaseball-reference.com/v1/playerStats?category=pitching&playerIds={}&season={}",
            comma_pitchers,
            data.value.games.sim.season
        ))?
        .json::<Vec<PitchingStats>>()?
        .into_iter()
        .max_by_key(|x| n64(x.k_per_9))
        .ok_or_else(|| anyhow!("No best pitcher!"))?;
    let (best_so9_game, best_so9_name) = data
        .value
        .games
        .tomorrow_schedule
        .iter()
        .filter_map(|x| {
            if x.away_pitcher == best_so9.player_id {
                Some((x, &x.away_pitcher_name))
            } else if x.home_pitcher == best_so9.player_id {
                Some((x, &x.home_pitcher_name))
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| anyhow!("Couldn't find name for best SO9 pitcher!"))?;
    let req = reqwest::blocking::get(&format!(
        "https://www.blaseball.com/database/players?ids={}",
        comma_pitchers
    ))?;
    let best_ruthlessness = req
        .json::<Vec<Player>>()?
        .into_iter()
        .max_by_key(|x| n64(x.ruthlessness))
        .ok_or_else(|| anyhow!("No best pitcher!"))?;
    let best_ruthlessness_game = data
        .value
        .games
        .tomorrow_schedule
        .iter()
        .filter_map(|x| {
            if x.away_pitcher == best_ruthlessness.id {
                Some(x)
            } else if x.home_pitcher == best_ruthlessness.id {
                Some(x)
            } else {
                None
            }
        })
        .next()
        .ok_or_else(|| anyhow!("Couldn't find name for best SO9 pitcher!"))?;
    let mut text = String::new();
    writeln!(text, "**Day {}**", data.value.games.sim.day + 2)?; // tomorrow, zero-indexed
    writeln!(
        text,
        "Best pitcher by SO/9: {} ({}, {} vs. {})",
        best_so9_name, best_so9.k_per_9, best_so9_game.away_team_name, best_so9_game.home_team_name
    )?;
    write!(
        text,
        "Best pitcher by ruthlessness: {} ({}, {} vs. {})",
        best_ruthlessness.name,
        best_ruthlessness.ruthlessness,
        best_ruthlessness_game.away_team_name,
        best_ruthlessness_game.home_team_name
    )?;
    Ok(Some(text))
}

fn send_hook(url: &str) -> Result<()> {
    let content = match get_best()? {
        Some(x) => x,
        None => return Ok(()),
    };
    println!("{}", content);
    let hook = Webhook { content: &content };
    reqwest::blocking::Client::new()
        .post(url)
        .json(&hook)
        .send()?;
    Ok(())
}

fn wait_for_next_game() {
    let now = Utc::now();
    let one_hour = chrono::Duration::hours(1);
    let later = now + one_hour;
    let game = later
        .with_minute(1)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();
    let time_till_game = game - now;
    println!("Sleeping for {} (until {})...", time_till_game, game);
    std::thread::sleep(time_till_game.to_std().unwrap());
}

fn main() -> anyhow::Result<()> {
    let url = dotenv::var("WEBHOOK_URL")?;

    loop {
        send_hook(&url)?;
        wait_for_next_game();
    }
}
