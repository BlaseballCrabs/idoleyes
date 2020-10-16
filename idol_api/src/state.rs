use super::models::{
    AtBatLeader, Event, Game, Idol, PitchingStats, Position, StrikeoutLeader, Team,
};
use anyhow::Result;
use log::*;
use serde::Deserialize;

#[derive(Debug)]
pub struct State {
    pub strikeouts: Vec<StrikeoutLeader>,
    pub at_bats: Vec<AtBatLeader>,
    pub pitcher_stats: Vec<PitchingStats>,
    pub teams: Vec<Team>,
    pub players: Vec<Position>,
    pub games: Vec<Game>,
    pub idols: Vec<Idol>,
}

impl State {
    pub fn from_event(data: Event) -> Result<Self> {
        Self::from_games_and_season(
            data.value.games.tomorrow_schedule,
            data.value.games.sim.season,
        )
    }

    pub fn from_games_and_season(games: Vec<Game>, season: usize) -> Result<Self> {
        #[derive(Deserialize)]
        struct Positions {
            data: Vec<Position>,
        }
        let client = reqwest::blocking::Client::new();
        let comma_pitchers = games
            .iter()
            .flat_map(Game::pitcher_ids)
            .collect::<Vec<&str>>()
            .join(",");
        debug!("Getting batter strikeouts");
        let strikeouts: Vec<StrikeoutLeader> = client
            .get(
                "https://api.blaseball-reference.com/v1/seasonLeaders?category=batting&stat=strikeouts",
            )
            .query(&[("season", season)])
            .send()?
            .json()?;
        debug!("Getting at-bats");
        let at_bats: Vec<AtBatLeader> = client
            .get("https://api.blaseball-reference.com/v1/seasonLeaders?category=batting&stat=at_bats")
            .query(&[("season", season)])
            .send()?
            .json()?;
        debug!("Getting pitcher stats");
        let pitcher_stats: Vec<PitchingStats> = client
            .get("https://api.blaseball-reference.com/v1/playerStats?category=pitching")
            .query(&[("playerIds", comma_pitchers)])
            .query(&[("season", season)])
            .send()?
            .json()?;
        debug!("Getting teams");
        let teams: Vec<Team> = client
            .get("https://www.blaseball.com/database/allTeams")
            .send()?
            .json()?;
        debug!("Getting players");
        let players = client
            .get("https://api.sibr.dev/chronicler/v1/players?forbidden=false")
            .send()?
            .json::<Positions>()?
            .data;
        let idols = client
            .get("https://www.blaseball.com/api/getIdols")
            .send()?
            .json::<Vec<Idol>>()?;
        Ok(Self {
            strikeouts,
            at_bats,
            pitcher_stats,
            teams,
            players,
            games,
            idols,
        })
    }
}
