use anyhow::{anyhow, Result};
use either::Either;
use join_lazy_fmt::{lazy_format, Join};
use log::*;
use noisy_float::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::mem::take;
use std::result::Result as StdResult;

pub mod algorithms;

#[derive(Copy, Clone)]
pub struct Algorithm {
    pub name: &'static str,
    pub forbidden: Forbidden,
    pub printed_stats: &'static [PrintedStat],
    pub strategy: fn(PitcherRef) -> Option<f64>,
}

impl Algorithm {
    pub fn best_pitcher(self, state: &State) -> Result<ScoredPitcher> {
        state.best_pitcher(self.strategy)
    }

    pub fn display<'a, 'b>(self, scored: &'a ScoredPitcher<'b>) -> impl fmt::Display + 'a {
        scored.display(self.name, self.forbidden, self.printed_stats)
    }

    pub fn write_best_to(self, state: &State, output: &mut impl fmt::Write) -> Result<()> {
        writeln!(output, "{}", self.display(&self.best_pitcher(state)?))?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PitchingStats {
    pub player_id: String,
    pub player_name: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub k_per_9: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StrikeoutLeader {
    pub player_id: String,
    #[serde(with = "serde_with::rust::display_fromstr")]
    pub strikeouts: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AtBatLeader {
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
pub struct Position {
    pub id: String,
    pub data: Player,
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

#[derive(Debug)]
pub struct State {
    pub strikeouts: Vec<StrikeoutLeader>,
    pub at_bats: Vec<AtBatLeader>,
    pub pitcher_stats: Vec<PitchingStats>,
    pub teams: Vec<Team>,
    pub players: Vec<Position>,
    pub games: Vec<Game>,
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
            .get("https://api.sibr.dev/chronicler/v1/players")
            .send()?
            .json::<Positions>()?
            .data;
        Ok(Self {
            strikeouts,
            at_bats,
            pitcher_stats,
            teams,
            players,
            games,
        })
    }

    pub fn best_pitcher<'a>(
        &'a self,
        mut strategy: impl FnMut(PitcherRef<'a>) -> Option<f64>,
    ) -> Result<ScoredPitcher<'a>> {
        self.games
            .iter()
            .filter_map(|game| game.pitchers(self))
            .flatten()
            .filter_map(|pitcher| {
                Some(ScoredPitcher {
                    pitcher,
                    score: strategy(pitcher)?,
                })
            })
            .max_by_key(|scored| n64(scored.score))
            .ok_or_else(|| anyhow!("No best pitcher!"))
    }
}

#[derive(Debug, Copy, Clone)]
pub enum TeamPosition {
    Home,
    Away,
}

#[derive(Debug, Copy, Clone)]
pub struct TeamPair<T> {
    pub home: T,
    pub away: T,
}

impl<T> TeamPair<T> {
    pub fn map<M, F>(self, mut func: F) -> TeamPair<M>
    where
        F: FnMut(T) -> M,
    {
        TeamPair {
            home: func(self.home),
            away: func(self.away),
        }
    }

    pub fn and_then<A, M, F>(self, mut func: F) -> M
    where
        F: FnMut(T) -> A,
        TeamPair<A>: Transpose<M>,
    {
        TeamPair {
            home: func(self.home),
            away: func(self.away),
        }
        .transpose()
    }

    pub fn map_both<M, F>(&self, mut func: F) -> TeamPair<M>
    where
        F: FnMut(&T, &T) -> M,
    {
        TeamPair {
            home: func(&self.home, &self.away),
            away: func(&self.away, &self.home),
        }
    }

    pub fn map_pos<M, F>(self, mut func: F) -> TeamPair<M>
    where
        F: FnMut(T, TeamPosition) -> M,
    {
        TeamPair {
            home: func(self.home, TeamPosition::Home),
            away: func(self.away, TeamPosition::Away),
        }
    }

    pub fn map_both_pos<M, F>(&self, mut func: F) -> TeamPair<M>
    where
        F: FnMut(&T, &T, TeamPosition) -> M,
    {
        TeamPair {
            home: func(&self.home, &self.away, TeamPosition::Home),
            away: func(&self.away, &self.home, TeamPosition::Away),
        }
    }

    pub fn as_ref(&self) -> TeamPair<&T> {
        TeamPair {
            home: &self.home,
            away: &self.away,
        }
    }

    pub fn as_mut(&mut self) -> TeamPair<&mut T> {
        TeamPair {
            home: &mut self.home,
            away: &mut self.away,
        }
    }

    pub fn zip<B>(self, other: TeamPair<B>) -> TeamPair<(T, B)> {
        TeamPair {
            home: (self.home, other.home),
            away: (self.away, other.away),
        }
    }
}

enum TeamPairPosition<T> {
    Home { home: T, away: T },
    Away { away: T },
    End,
}

impl<T> Default for TeamPairPosition<T> {
    fn default() -> Self {
        Self::End
    }
}

pub struct TeamPairIntoIter<T> {
    position: TeamPairPosition<T>,
}

impl<T> Iterator for TeamPairIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        use TeamPairPosition::*;
        let (new, res) = match take(&mut self.position) {
            Home { home, away } => (Away { away }, Some(home)),
            Away { away } => (End, Some(away)),
            End => (End, None),
        };
        self.position = new;
        res
    }
}

impl<T> IntoIterator for TeamPair<T> {
    type Item = T;
    type IntoIter = TeamPairIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        TeamPairIntoIter {
            position: TeamPairPosition::Home {
                home: self.home,
                away: self.away,
            },
        }
    }
}

impl<'a, T> IntoIterator for &'a TeamPair<T> {
    type Item = &'a T;
    type IntoIter = TeamPairIntoIter<&'a T>;

    fn into_iter(self) -> Self::IntoIter {
        TeamPairIntoIter {
            position: TeamPairPosition::Home {
                home: &self.home,
                away: &self.away,
            },
        }
    }
}

pub trait Transpose<T> {
    fn transpose(self) -> T;
}

impl<T> Transpose<Option<TeamPair<T>>> for TeamPair<Option<T>> {
    fn transpose(self) -> Option<TeamPair<T>> {
        Some(TeamPair {
            home: self.home?,
            away: self.away?,
        })
    }
}

impl<T, E> Transpose<StdResult<TeamPair<T>, E>> for TeamPair<StdResult<T, E>> {
    fn transpose(self) -> StdResult<TeamPair<T>, E> {
        Ok(TeamPair {
            home: self.home?,
            away: self.away?,
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct PitcherRef<'a> {
    pub id: &'a str,
    pub position: &'a Position,
    pub player: &'a Player,
    pub stats: &'a PitchingStats,
    pub game: &'a Game,
    pub state: &'a State,
    pub team: &'a Team,
    pub opponent: &'a Team,
    pub team_pos: TeamPosition,
}

impl Game {
    pub fn pitcher_ids(&self) -> TeamPair<&str> {
        TeamPair {
            home: &self.home_pitcher,
            away: &self.away_pitcher,
        }
    }

    pub fn team_ids(&self) -> TeamPair<&str> {
        TeamPair {
            home: &self.home_team,
            away: &self.away_team,
        }
    }

    pub fn teams<'a>(&self, state: &'a State) -> Option<TeamPair<&'a Team>> {
        self.team_ids()
            .and_then(|x| state.teams.iter().find(|y| x == y.id))
    }

    pub fn pitcher_positions<'a>(&self, state: &'a State) -> Option<TeamPair<&'a Position>> {
        self.pitcher_ids()
            .and_then(|x| state.players.iter().find(|y| x == y.id))
    }

    pub fn pitcher_stats<'a>(&self, state: &'a State) -> Option<TeamPair<&'a PitchingStats>> {
        self.pitcher_ids()
            .and_then(|x| state.pitcher_stats.iter().find(|y| x == y.player_id))
    }

    pub fn pitchers<'a>(&'a self, state: &'a State) -> Option<TeamPair<PitcherRef<'a>>> {
        Some(
            self.pitcher_positions(state)?
                .zip(self.pitcher_stats(state)?)
                .zip(self.teams(state)?)
                .map_both_pos(
                    |&((position, stats), team), &(_, opponent), team_pos| PitcherRef {
                        id: &position.id,
                        position,
                        player: &position.data,
                        stats,
                        game: self,
                        state,
                        team,
                        opponent,
                        team_pos,
                    },
                ),
        )
    }
}

impl Team {
    pub fn at_bats<'a>(&'a self, state: &'a State) -> impl Iterator<Item = Option<usize>> + 'a {
        self.lineup.iter().map(move |x| {
            state
                .at_bats
                .iter()
                .find(|y| x == &y.player_id)
                .map(|y| y.at_bats)
        })
    }

    pub fn strikeouts<'a>(&'a self, state: &'a State) -> impl Iterator<Item = Option<usize>> + 'a {
        self.lineup.iter().map(move |x| {
            state
                .strikeouts
                .iter()
                .find(|y| x == &y.player_id)
                .map(|y| y.strikeouts)
        })
    }
}

#[derive(Debug, Copy, Clone)]
pub struct ScoredPitcher<'a> {
    pub pitcher: PitcherRef<'a>,
    pub score: f64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Forbidden {
    Forbidden,
    Unforbidden,
}

impl Forbidden {
    fn forbid<'a>(self, text: impl fmt::Display + 'a) -> impl fmt::Display + 'a {
        if self == Self::Forbidden {
            Either::Left(lazy_format!("||{}||", text))
        } else {
            Either::Right(text)
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum PrintedStat {
    SO9,
}

impl PrintedStat {
    fn print(self, pitcher: PitcherRef) -> impl fmt::Display + '_ {
        match self {
            Self::SO9 => lazy_format!("SO/9: {}", pitcher.stats.k_per_9),
        }
    }
}

impl ScoredPitcher<'_> {
    pub fn display<'a>(
        &'a self,
        strategy: &'a str,
        forbidden: Forbidden,
        stats: &'a [PrintedStat],
    ) -> impl fmt::Display + 'a {
        let printed_stats = "".join(
            stats
                .iter()
                .map(move |stat| lazy_format!(", {}", stat.print(self.pitcher))),
        );
        let versus = match self.pitcher.team_pos {
            TeamPosition::Home => "vs.",
            TeamPosition::Away => "@",
        };
        let knowledge = forbidden.forbid(lazy_format!(
            "{strategy}: {name} ({score:.3}{stats}, **{team}** {versus} {opponent})",
            strategy = strategy,
            name = self.pitcher.player.name,
            stats = printed_stats,
            score = self.score,
            team = self.pitcher.team.full_name,
            versus = versus,
            opponent = self.pitcher.opponent.full_name
        ));
        lazy_format!("Best by {}", knowledge)
    }
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

#[derive(Debug, Serialize, Deserialize)]
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
