#![feature(array_value_iter)]

use anyhow::{anyhow, Result};
use chrono::prelude::*;
use eventsource::reqwest::Client;
use fallible_iterator::FallibleIterator;
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
    pub patheticism: f64,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub lineup: Vec<String>,
    pub rotation: Vec<String>,
    pub bullpen: Vec<String>,
    pub bench: Vec<String>,
    pub perm_attr: Vec<String>,
}

fn get_batters_and_team(team_id: &str) -> Result<(Vec<Player>, Team)> {
    let client = reqwest::blocking::Client::new();
    let team: Team = client
        .get("https://www.blaseball.com/database/team")
        .query(&[("id", team_id)])
        .send()?
        .json()?;
    let batter_ids = team.lineup.join(",");
    let batters: Vec<Player> = client
        .get("https://www.blaseball.com/database/players")
        .query(&[("ids", batter_ids)])
        .send()?
        .json()?;
    Ok((batters, team))
}

fn adjust_patheticism(player: &Player, team: &Team) -> f64 {
    let base = player.patheticism;
    let inverted = 1.0 - base;
    let electric = team.perm_attr.iter().any(|x| x == "ELECTRIC");
    if electric {
        inverted
    } else {
        inverted / 2.0
    }
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
    ))?
    .json::<Vec<Player>>()?;
    let best_ruthlessness = req
        .iter()
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
    let (best_ratio_player, best_ratio_game, best_ratio) =
        fallible_iterator::convert(data.value.games.tomorrow_schedule.iter().map(
            |x| -> Result<_> {
                let (away_batters, away_team) = get_batters_and_team(&x.away_team)?;
                let (home_batters, home_team) = get_batters_and_team(&x.home_team)?;
                let away_patheticisms: Vec<_> = away_batters
                    .iter()
                    .map(|x| adjust_patheticism(x, &away_team))
                    .collect();
                let home_patheticisms: Vec<_> = home_batters
                    .iter()
                    .map(|x| adjust_patheticism(x, &home_team))
                    .collect();
                let away_patheticism = away_patheticisms
                    .iter()
                    .product::<f64>()
                    .powf(1.0 / away_patheticisms.len() as f64);
                let home_patheticism = home_patheticisms
                    .iter()
                    .product::<f64>()
                    .powf(1.0 / home_patheticisms.len() as f64);
                let away_pitcher = req
                    .iter()
                    .find(|y| y.id == x.away_pitcher)
                    .ok_or_else(|| anyhow!("Missing pitcher!"))?;
                let home_pitcher = req
                    .iter()
                    .find(|y| y.id == x.home_pitcher)
                    .ok_or_else(|| anyhow!("Missing pitcher!"))?;
                let away_ruthlessness = (away_pitcher.ruthlessness * 4.0).atan() / 1.5;
                let home_ruthlessness = (home_pitcher.ruthlessness * 4.0).atan() / 1.5;
                let away_ratio = away_ruthlessness / home_patheticism;
                let home_ratio = home_ruthlessness / away_patheticism;
                if away_ratio > home_ratio {
                    Ok((away_pitcher, x, away_ratio))
                } else {
                    Ok((home_pitcher, x, away_ratio))
                }
            },
        ))
        .max_by_key(|x| Ok(n64(x.2)))?
        .ok_or_else(|| anyhow!("No best pitcher!"))?;
    let mut text = String::new();
    writeln!(text, "**Day {}**", data.value.games.sim.day + 2)?; // tomorrow, zero-indexed
    let so9_away = if best_so9.player_id == best_so9_game.away_pitcher {
        format!("**{}**", best_so9_game.away_team_name)
    } else {
        best_so9_game.away_team_name.clone()
    };
    let so9_home = if best_so9.player_id == best_so9_game.home_pitcher {
        format!("**{}**", best_so9_game.home_team_name)
    } else {
        best_so9_game.home_team_name.clone()
    };
    writeln!(
        text,
        "Best pitcher by SO/9: {} ({}, {} vs. {})",
        best_so9_name, best_so9.k_per_9, so9_away, so9_home
    )?;
    let ruthlessness_away = if best_ruthlessness.id == best_ruthlessness_game.away_pitcher {
        format!("**{}**", best_ruthlessness_game.away_team_name)
    } else {
        best_ruthlessness_game.away_team_name.clone()
    };
    let ruthlessness_home = if best_ruthlessness.id == best_ruthlessness_game.home_pitcher {
        format!("**{}**", best_ruthlessness_game.home_team_name)
    } else {
        best_ruthlessness_game.home_team_name.clone()
    };
    writeln!(
        text,
        "Best pitcher by ||ruthlessness: {} ({}, {} vs. {})||",
        best_ruthlessness.name,
        best_ruthlessness.ruthlessness,
        ruthlessness_away,
        ruthlessness_home
    )?;
    let ratio_away = if best_ratio_player.id == best_ratio_game.away_pitcher {
        format!("**{}**", best_ratio_game.away_team_name)
    } else {
        best_ratio_game.away_team_name.clone()
    };
    let ratio_home = if best_ratio_player.id == best_ratio_game.home_pitcher {
        format!("**{}**", best_ratio_game.home_team_name)
    } else {
        best_ratio_game.home_team_name.clone()
    };
    write!(
        text,
        "(EXPERIMENTAL) Best pitcher by ||ruthlessness/patheticism: {} ({}, {} vs. {})||",
        best_ratio_player.name, best_ratio, ratio_away, ratio_home,
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
