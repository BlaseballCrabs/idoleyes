use super::{Algorithm, Forbidden::*, PitcherRef, PrintedStat, ScoredPitcher, Strategy::*};
use anyhow::anyhow;
use average::Mean;
use idol_api::team_pair::TeamPosition;
use noisy_float::prelude::*;
use paste::paste;

macro_rules! algorithm {
    ($id:ident, _, [$($stat:ident),*], $forbidden:ident, $($strat:tt)*) => {
        paste! {
            algorithm!($id, stringify!([<$id:lower>]), [$($stat),*], $forbidden, $($strat)*);
        }
    };

    ($id:ident, $name:expr, [$($stat:ident),*], $forbidden:ident, $($strat:tt)*) => {
        algorithm!($id, @ concat!("Best by ", $name), [$($stat),*], $forbidden, $($strat)*);
    };

    ($id:ident, @ $name:expr, [$($stat:ident),*], $forbidden:ident, |$x:ident| $strat:expr) => {
        paste! {
            pub fn [<best_by_ $id:lower>]($x: PitcherRef) -> Option<f64> {
                Some($strat)
            }

            algorithm!($id, @ $name, [$($stat),*], $forbidden, Maximize([<best_by_ $id:lower>]));
        }
    };

    ($id:ident, @ $name:expr, [$($stat:ident),*], $forbidden:ident, $strat:expr) => {
        pub const $id: Algorithm = Algorithm {
            name: $name,
            forbidden: $forbidden,
            printed_stats: &[$(PrintedStat::$stat),*],
            strategy: $strat,
        };
    };
}

algorithm!(SO9, "SO/9", [], Unforbidden, |x| x.stats?.k_per_9);

algorithm!(RUTHLESSNESS, _, [SO9], Forbidden, |x| x.player.ruthlessness);

algorithm!(STAT_RATIO, "(SO/9)(SO/AB)", [SO9], Unforbidden, |x| {
    x.stats?.k_per_9
        * (0.2
            + x.opponent
                .strikeouts(x.state)
                .zip(x.opponent.at_bats(x.state))
                .map(|(so, ab)| Some((so?, ab?)))
                .map(|x| x.map(|(so, ab)| so as f64 / ab as f64))
                .collect::<Option<Mean>>()?
                .mean())
});

algorithm!(
    BESTNESS,
    "Bestness",
    [],
    Unforbidden,
    Custom(|state| {
        let (position, score) = state
            .players
            .iter()
            .filter(|x| x.data.name.contains("Best"))
            .map(|x| (x, 4.0 / x.data.name.len() as f64))
            .max_by_key(|x| n64(x.1))
            .ok_or_else(|| anyhow!("No Best player!"))?;
        let game = state
            .games
            .iter()
            .find(|x| x.home_team == position.team_id || x.away_team == position.team_id)
            .ok_or_else(|| anyhow!("No Crabs game!"))?;
        let teams = game
            .teams(state)
            .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
        let (team, opponent, team_pos) = if teams.away.id == position.team_id {
            (teams.away, teams.home, TeamPosition::Away)
        } else {
            (teams.home, teams.away, TeamPosition::Home)
        };
        let id = &position.data.id;
        let player = &position.data;
        let pitcher = PitcherRef {
            id,
            position,
            player,
            stats: None,
            game,
            state,
            team,
            opponent,
            team_pos,
        };
        Ok(ScoredPitcher { pitcher, score })
    })
);

algorithm!(
    BEST_BEST,
    @ "Best Best by Stars",
    [],
    Unforbidden,
    Custom(|state| {
        let (position, score) = state
            .players
            .iter()
            .filter(|x| x.data.name.contains("Best"))
            .map(|x| (x, (x.data.pitching_rating * 10.0).floor() / 2.0))
            .max_by_key(|x| n64(x.1))
            .ok_or_else(|| anyhow!("No Best player!"))?;
        let game = state
            .games
            .iter()
            .find(|x| x.home_team == position.team_id || x.away_team == position.team_id)
            .ok_or_else(|| anyhow!("No Crabs game!"))?;
        let teams = game
            .teams(state)
            .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
        let (team, opponent, team_pos) = if teams.away.id == position.team_id {
            (teams.away, teams.home, TeamPosition::Away)
        } else {
            (teams.home, teams.away, TeamPosition::Home)
        };
        let id = &position.data.id;
        let player = &position.data;
        let pitcher = PitcherRef {
            id,
            position,
            player,
            stats: None,
            game,
            state,
            team,
            opponent,
            team_pos,
        };
        Ok(ScoredPitcher { pitcher, score })
    })
);

const FRIDAYS_ID: &str = "979aee4a-6d80-4863-bf1c-ee1a78e06024";

algorithm!(FRIDAYS, @ "Against Fridays", [], Unforbidden, |x| if x.opponent.id == FRIDAYS_ID { 1.0 } else { 0.0 });

algorithm!(WORST_STAT_RATIO, @ "Worst by (-SO/9)/(SO/AB)", [SO9], Unforbidden, |x| {
    -x.stats?.k_per_9
        / x.opponent
                .strikeouts(x.state)
                .zip(x.opponent.at_bats(x.state))
                .map(|(so, ab)| Some((so?, ab?)))
                .map(|x| x.map(|(so, ab)| so as f64 / ab as f64))
                .collect::<Option<Mean>>()?
                .mean()
});

algorithm!(IDOLS, "idolization", [], Unforbidden, |x| {
    -(x.state
        .idols
        .iter()
        .position(|y| y.player_id == x.player.id)
        .unwrap_or(20) as f64)
        - 1.0
});

algorithm!(BATTING_STARS, "batting stars", [], Unforbidden, |x| {
    (x.player.hitting_rating * 10.0).floor() / 2.0
});

algorithm!(NAME_LENGTH, "name length", [], Unforbidden, |x| {
    x.player.name.len() as f64
});

pub const ALGORITHMS: &[Algorithm] = &[SO9, RUTHLESSNESS, STAT_RATIO];

pub const JOKE_ALGORITHMS: &[Algorithm] = &[
    BESTNESS,
    BEST_BEST,
    FRIDAYS,
    WORST_STAT_RATIO,
    IDOLS,
    BATTING_STARS,
    NAME_LENGTH,
];
