use anyhow::{anyhow, Result};
use either::Either;
use idol_api::models::{Game, PitchingStats, Player, Position, Team};
use idol_api::team_pair::{TeamPair, TeamPosition};
use idol_api::State;
use join_lazy_fmt::{lazy_format, Join};
use noisy_float::prelude::*;
use std::fmt;

pub mod algorithms;

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

impl<'a> PitcherRef<'a> {
    pub fn pitchers(game: &'a Game, state: &'a State) -> Option<TeamPair<Self>> {
        Some(
            game.pitcher_positions(state)?
                .zip(game.pitcher_stats(state)?)
                .zip(game.teams(state)?)
                .map_both_pos(
                    |&((position, stats), team), &(_, opponent), team_pos| PitcherRef {
                        id: &position.id,
                        position,
                        player: &position.data,
                        stats,
                        game,
                        state,
                        team,
                        opponent,
                        team_pos,
                    },
                ),
        )
    }
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

#[derive(Debug, Copy, Clone)]
pub struct ScoredPitcher<'a> {
    pub pitcher: PitcherRef<'a>,
    pub score: f64,
}

impl<'a> ScoredPitcher<'a> {
    pub fn best_pitcher(
        state: &'a State,
        mut strategy: impl FnMut(PitcherRef<'a>) -> Option<f64>,
    ) -> Result<Self> {
        state
            .games
            .iter()
            .filter_map(|game| PitcherRef::pitchers(game, state))
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

    pub fn display<'b>(
        &'b self,
        strategy: &'b str,
        forbidden: Forbidden,
        stats: &'b [PrintedStat],
    ) -> impl fmt::Display + 'b {
        let printed_stats = "".join(
            stats
                .iter()
                .map(move |stat| lazy_format!(", {}", stat.print(self.pitcher))),
        );
        let versus = match self.pitcher.team_pos {
            TeamPosition::Home => "vs.",
            TeamPosition::Away => "@",
        };
        forbidden.forbid(lazy_format!(
            "{strategy}: {name} ({score:.3}{stats}, **{team}** {versus} {opponent})",
            strategy = strategy,
            name = self.pitcher.player.name,
            stats = printed_stats,
            score = self.score,
            team = self.pitcher.team.full_name,
            versus = versus,
            opponent = self.pitcher.opponent.full_name
        ))
    }
}

#[derive(Copy, Clone)]
pub struct Algorithm {
    pub name: &'static str,
    pub forbidden: Forbidden,
    pub printed_stats: &'static [PrintedStat],
    pub strategy: fn(PitcherRef) -> Option<f64>,
}

impl Algorithm {
    pub fn best_pitcher(self, state: &State) -> Result<ScoredPitcher> {
        ScoredPitcher::best_pitcher(state, self.strategy)
    }

    pub fn display<'a, 'b>(self, scored: &'a ScoredPitcher<'b>) -> impl fmt::Display + 'a {
        scored.display(self.name, self.forbidden, self.printed_stats)
    }

    pub fn write_best_to(self, state: &State, output: &mut impl fmt::Write) -> Result<()> {
        writeln!(output, "{}", self.display(&self.best_pitcher(state)?))?;
        Ok(())
    }
}
