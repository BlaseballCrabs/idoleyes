use super::models::{
    AtBatLeader, Event, Game, GameUpdate, GameUpdates, Idol, Idols, PitchingStats, Position,
    StrikeoutLeader, Team,
};
use anyhow::Result;
use log::*;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct State {
    pub strikeouts: Vec<StrikeoutLeader>,
    pub at_bats: Vec<AtBatLeader>,
    pub pitcher_stats: Vec<PitchingStats>,
    pub teams: Vec<Team>,
    pub players: Vec<Position>,
    pub games: Vec<Game>,
    pub idols: Vec<Idol>,
    pub black_hole_sun_2: Vec<GameUpdate>,
    pub season: isize,
}

impl State {
    pub async fn from_event(data: &Event) -> Result<Self> {
        let games = if data.value.games.tomorrow_schedule.is_empty() {
            warn!("No games scheduled, checking current games");
            data.value.games.schedule.clone()
        } else {
            data.value.games.tomorrow_schedule.clone()
        };
        Self::from_games_and_season(games, data.value.games.sim.season).await
    }

    pub async fn from_games_and_season(games: Vec<Game>, season: isize) -> Result<Self> {
        #[derive(Deserialize)]
        struct Positions {
            data: Vec<Position>,
        }
        let client = surf::Client::new();
        let comma_pitchers = games
            .iter()
            .filter_map(Game::pitcher_ids)
            .flatten()
            .collect::<Vec<&str>>()
            .join(",");
        debug!("Getting batter strikeouts");

        #[derive(Serialize)]
        struct LeadersQuery {
            category: &'static str,
            stat: &'static str,
            season: isize,
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct StatsQuery {
            category: &'static str,
            player_ids: String,
            season: isize,
        }

        let strikeouts: Vec<StrikeoutLeader> = client
            .get("https://api.blaseball-reference.com/v1/seasonLeaders")
            .query(&LeadersQuery {
                category: "batting",
                stat: "strikeouts",
                season,
            })
            .map_err(|x| x.into_inner())?
            .await
            .map_err(|x| x.into_inner())?
            .body_json()
            .await
            .unwrap_or_else(|_| Vec::new());
        debug!("Getting at-bats");
        let at_bats: Vec<AtBatLeader> = client
            .get("https://api.blaseball-reference.com/v1/seasonLeaders")
            .query(&LeadersQuery {
                category: "batting",
                stat: "at_bats",
                season,
            })
            .map_err(|x| x.into_inner())?
            .send()
            .await
            .map_err(|x| x.into_inner())?
            .body_json()
            .await
            .unwrap_or_else(|_| Vec::new());
        debug!("Getting pitcher stats");
        let pitcher_stats: Vec<PitchingStats> = client
            .get("https://api.blaseball-reference.com/v1/playerStats")
            .query(&StatsQuery {
                category: "pitching",
                player_ids: comma_pitchers,
                season,
            })
            .map_err(|x| x.into_inner())?
            .send()
            .await
            .map_err(|x| x.into_inner())?
            .body_json()
            .await
            .unwrap_or_else(|_| Vec::new());
        debug!("Getting teams");
        let teams: Vec<Team> = client
            .get("https://www.blaseball.com/database/allTeams")
            .send()
            .await
            .map_err(|x| x.into_inner())?
            .body_json()
            .await
            .map_err(|x| x.into_inner())?;
        debug!("Getting players");
        let players = client
            .get("https://api.sibr.dev/chronicler/v1/players?forbidden=false")
            .send()
            .await
            .map_err(|x| x.into_inner())?
            .body_json::<Positions>()
            .await
            .map_err(|x| x.into_inner())?
            .data;
        debug!("Getting Sun 2 and Black Hole events");
        let black_hole_sun_2 = client
            .get("https://api.sibr.dev/chronicler/v1/games/updates?search=%22Sun%202%22%20or%20%22Black%20Hole%22&count=1000&order=desc")
            .send()
            .await.map_err(|x| x.into_inner())?
            .body_json::<GameUpdates>()
            .await.map_err(|x| x.into_inner())?
            .data;
        let idols = client
            .get("https://www.blaseball.com/api/getIdols")
            .send()
            .await
            .map_err(|x| x.into_inner())?
            .body_json::<Idols>()
            .await
            .map_err(|x| x.into_inner())?
            .idols;
        Ok(Self {
            strikeouts,
            at_bats,
            pitcher_stats,
            teams,
            players,
            games,
            idols,
            black_hole_sun_2,
            season,
        })
    }
}
