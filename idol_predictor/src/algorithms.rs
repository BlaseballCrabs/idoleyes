use super::{Algorithm, Forbidden::*, PitcherRef, PrintedStat, ScoredPitcher, Strategy::*};
use anyhow::{anyhow, bail};
use average::Mean;
use idol_api::models::{Game, Team};
use idol_api::team_pair::TeamPosition;
use noisy_float::prelude::*;
use paste::paste;
use std::borrow::Cow;

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

algorithm!(SO9, "SO/9", [], Unforbidden, |x| x.stats?.strikeouts_per_9);

algorithm!(RUTHLESSNESS, _, [SO9], Forbidden, |x| x.player.ruthlessness);

algorithm!(STAT_RATIO, "(SO/9)(SO/AB)", [SO9], Unforbidden, |x| {
    x.stats?.strikeouts_per_9
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
            .find(|x| {
                x.home_team == position.data.league_team_id
                    || x.away_team == position.data.league_team_id
            })
            .ok_or_else(|| anyhow!("No Best game!"))?;
        let teams = game
            .teams(state)
            .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
        let (team, opponent, team_pos) = if teams.away.id == position.data.league_team_id {
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

const CRABS: &str = "8d87c468-699a-47a8-b40d-cfb73a5660ad";

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
        let game = state.games.iter().find(|x| {
            x.home_team == position.data.league_team_id
                || x.away_team == position.data.league_team_id
        });
        if let Some(game) = game {
            let teams = game
                .teams(state)
                .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
            let (team, opponent, team_pos) = if teams.away.id == position.data.league_team_id {
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
        } else if position.data.league_team_id == CRABS {
            let team = state
                .teams
                .iter()
                .find(|x| x.id == CRABS)
                .ok_or_else(|| anyhow!("Missing Crabs!"))?;
            let id = &position.data.id;
            let player = &position.data;
            static BLACK_HOLE: Team = Team {
                id: String::new(),
                full_name: Cow::Borrowed("Black Hole"),
                lineup: Vec::new(),
                rotation: Vec::new(),
                bullpen: Vec::new(),
                bench: Vec::new(),
                perm_attr: Vec::new(),
            };
            let team_pos = TeamPosition::Away;
            static GAME: Game = Game {
                id: String::new(),
                away_pitcher: None,
                away_pitcher_name: None,
                home_pitcher: None,
                home_pitcher_name: None,
                away_team: Cow::Borrowed(&CRABS),
                away_team_name: Cow::Borrowed("Baltimore Crabs"),
                home_team: String::new(),
                home_team_name: Cow::Borrowed("Black Hole"),
                away_odds: 0.0,
                home_odds: 0.0,
                inning: 0,
                day: 0,
                season: 0,
            };
            let pitcher = PitcherRef {
                id,
                position,
                player,
                stats: None,
                game: &GAME,
                state,
                team,
                opponent: &BLACK_HOLE,
                team_pos,
            };
            Ok(ScoredPitcher { pitcher, score })
        } else {
            bail!("No Best game!");
        }
    })
);

const LIFT_ID: &str = "c73b705c-40ad-4633-a6ed-d357ee2e2bcf";

algorithm!(LIFT, @ "Against Lift", [], Unforbidden, |x| if x.opponent.id == LIFT_ID { 1.0 } else { 0.0 });

algorithm!(WORST_STAT_RATIO, @ "Worst by (-SO/9)/(SO/AB)", [SO9], Unforbidden, |x| {
    -x.stats?.strikeouts_per_9
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

algorithm!(GAMES_PER_GAME, "games per game", [], Unforbidden, |x| {
    let normal_games = x.stats?.games;
    let extra = x
        .state
        .sun_2
        .iter()
        .chain(&x.state.black_hole)
        .take_while(|y| y.season == x.state.season)
        .filter_map(|item| {
            let team = &item.team_tags[0];
            let game = item.game(&x.state)?;
            let pitchers = game.pitcher_ids()?;
            Some((team, game, pitchers))
        })
        .filter(|(team, game, pitchers)| {
            if *team == &game.home_team {
                pitchers.home == x.id
            } else {
                pitchers.away == x.id
            }
        })
        .count();
    let games = normal_games + extra;
    games as f64 / normal_games as f64
});

algorithm!(
    GAMES_NAME_PER_GAME,
    "Games per game",
    [],
    Unforbidden,
    Custom(|state| {
        let game = state
            .games
            .iter()
            .find(|x| {
                x.pitcher_names()
                    .map(|y| y.any(|z| z.contains("Games")))
                    .unwrap_or(false)
            })
            .ok_or_else(|| anyhow!("No Games game!"))?;
        let position = game
            .pitcher_positions(state)
            .into_iter()
            .flatten()
            .find(|x| x.data.name.contains("Games"))
            .ok_or_else(|| anyhow!("Lost the Games!"))?;
        let teams = game
            .teams(state)
            .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
        let (team, opponent, team_pos) = if teams.away.id == position.data.league_team_id {
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
        Ok(ScoredPitcher {
            pitcher,
            score: 1.0,
        })
    })
);

algorithm!(
    BATTING_MULTIPLIER,
    @ "Best Batter by Multiplier",
    [],
    Unforbidden,
    Custom(|state| {
        let (position, score) = state
            .players
            .iter()
            .filter_map(|x| match &x.data.name[..] {
                "York Silk" => Some((x, 2.0)),
                "Wyatt Glover" => Some((x, 5.0)),
                _ => None,
            })
            .max_by_key(|x| n64(x.1))
            .ok_or_else(|| anyhow!("No best multiplier!"))?;
        let game = state
            .games
            .iter()
            .find(|x| x.home_team == position.data.league_team_id || x.away_team == position.data.league_team_id)
            .ok_or_else(|| anyhow!("No game with {}!", position.data.name))?;
        let teams = game
            .teams(state)
            .ok_or_else(|| anyhow!("Couldn't get teams!"))?;
        let (team, opponent, team_pos) = if teams.away.id == position.data.league_team_id {
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

macro_rules! eat {
    ($x:expr) => {};
}

macro_rules! algorithms {
    (const ALGORITHMS = [$($serious:expr),*]; const JOKE_ALGORITHMS = [$($jokes:expr),*$(,)?];) => {
        pub const ALL_ALGORITHMS: &[Algorithm] = &[
            $($serious, )*
            $($jokes, )*
        ];

        #[allow(clippy::eval_order_dependence, unused_assignments)]
        pub const ALGORITHMS: &[i64] = {
            let mut i = 0;
            &[
                $({
                    eat!($serious);
                    let val = i;
                    i += 1;
                    val
                }),*
            ]
        };

        #[allow(clippy::eval_order_dependence, unused_assignments)]
        pub const JOKE_ALGORITHMS: &[i64] = {
            let mut i = ALGORITHMS.len() as i64;
            &[
                $({
                    eat!($jokes);
                    let val = i;
                    i += 1;
                    val
                }),*
            ]
        };
    };
}

algorithms! {
    const ALGORITHMS = [SO9, RUTHLESSNESS, STAT_RATIO];

    const JOKE_ALGORITHMS = [
        LIFT,
        BESTNESS,
        BEST_BEST,
        WORST_STAT_RATIO,
        IDOLS,
        BATTING_STARS,
        NAME_LENGTH,
        GAMES_PER_GAME,
        GAMES_NAME_PER_GAME,
        BATTING_MULTIPLIER,
    ];
}
