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
    pub player_name: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub k_per_9: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StrikeoutLeaders {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub strikeouts: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AtBatLeaders {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub at_bats: usize,
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

fn get_team(team_id: &str) -> Result<Team> {
    let client = reqwest::blocking::Client::new();
    let team: Team = client
        .get("https://www.blaseball.com/database/team")
        .query(&[("id", team_id)])
        .send()?
        .json()?;
    Ok(team)
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
    let pitcher_stats: Vec<PitchingStats> =
        reqwest::blocking::get(&format!(
            "https://api.blaseball-reference.com/v1/playerStats?category=pitching&playerIds={}&season={}",
            comma_pitchers,
            data.value.games.sim.season
        ))?
        .json()?;
    let best_so9 = pitcher_stats
        .iter()
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
    let best_ruthlessness_so9 = pitcher_stats
        .iter()
        .find(|x| x.player_id == best_ruthlessness.id)
        .ok_or_else(|| anyhow!("Lost player!"))?
        .k_per_9;
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
    let client = reqwest::blocking::Client::new();
    let strikeouts: Vec<StrikeoutLeaders> = client
        .get(
            "https://api.blaseball-reference.com/v1/seasonLeaders?category=batting&stat=strikeouts",
        )
        .query(&[("season", data.value.games.sim.season)])
        .send()?
        .json()?;
    let at_bats: Vec<AtBatLeaders> = client
        .get("https://api.blaseball-reference.com/v1/seasonLeaders?category=batting&stat=at_bats")
        .query(&[("season", data.value.games.sim.season)])
        .send()?
        .json()?;
    let (best_stat_ratio_player, best_stat_ratio_game, best_stat_ratio) =
        fallible_iterator::convert(data.value.games.tomorrow_schedule.iter().map(
            |x| -> Result<_> {
                let away_team = get_team(&x.away_team)?;
                let home_team = get_team(&x.home_team)?;
                let away_strikeouts = away_team.lineup.iter().map(|x| {
                    strikeouts
                        .iter()
                        .find(|y| &**x == &*y.player_id)
                        .unwrap()
                        .strikeouts
                });
                let away_at_bats = away_team.lineup.iter().map(|x| {
                    at_bats
                        .iter()
                        .find(|y| &**x == &*y.player_id)
                        .unwrap()
                        .at_bats
                });
                let away_soabs: Vec<_> = away_strikeouts
                    .zip(away_at_bats)
                    .map(|(so, ab)| so as f64 / ab as f64)
                    .collect();
                let home_strikeouts = home_team.lineup.iter().map(|x| {
                    strikeouts
                        .iter()
                        .find(|y| &**x == &*y.player_id)
                        .unwrap()
                        .strikeouts
                });
                let home_at_bats = home_team.lineup.iter().map(|x| {
                    at_bats
                        .iter()
                        .find(|y| &**x == &*y.player_id)
                        .unwrap()
                        .at_bats
                });
                let home_soabs: Vec<_> = home_strikeouts
                    .zip(home_at_bats)
                    .map(|(so, ab)| so as f64 / ab as f64)
                    .collect();
                let away_soab = away_soabs
                    .iter()
                    .product::<f64>()
                    .powf(1.0 / away_soabs.len() as f64);
                let home_soab = home_soabs
                    .iter()
                    .product::<f64>()
                    .powf(1.0 / home_soabs.len() as f64);
                let away_pitcher = pitcher_stats
                    .iter()
                    .find(|y| y.player_id == x.away_pitcher)
                    .ok_or_else(|| anyhow!("Missing pitcher!"))?;
                let home_pitcher = pitcher_stats
                    .iter()
                    .find(|y| y.player_id == x.home_pitcher)
                    .ok_or_else(|| anyhow!("Missing pitcher!"))?;
                let away_ratio = away_pitcher.k_per_9 / home_soab;
                let home_ratio = home_pitcher.k_per_9 / away_soab;
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
        "Best pitcher by ||ruthlessness: {} ({:.3}, SO/9: {}, {} vs. {})||",
        best_ruthlessness.name,
        best_ruthlessness.ruthlessness,
        best_ruthlessness_so9,
        ruthlessness_away,
        ruthlessness_home
    )?;
    let stat_ratio_away = if best_stat_ratio_player.player_id == best_stat_ratio_game.away_pitcher {
        format!("**{}**", best_stat_ratio_game.away_team_name)
    } else {
        best_stat_ratio_game.away_team_name.clone()
    };
    let stat_ratio_home = if best_stat_ratio_player.player_id == best_stat_ratio_game.home_pitcher {
        format!("**{}**", best_stat_ratio_game.home_team_name)
    } else {
        best_stat_ratio_game.home_team_name.clone()
    };
    write!(
        text,
        "Best pitcher by (SO/9)/(SO/AB): {} ({}, {} vs. {})",
        best_stat_ratio_player.player_name, best_stat_ratio, stat_ratio_away, stat_ratio_home,
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
