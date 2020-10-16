use super::{Algorithm, Forbidden::*, PitcherRef, PrintedStat};
use average::Mean;
use paste::paste;

macro_rules! algorithm {
    ($id:ident, _, [$($stat:ident),*], $forbidden:ident, |$x:ident| $strat:expr) => {
        paste! {
            algorithm!($id, stringify!([<$id:lower>]), [$($stat),*], $forbidden, |$x| $strat);
        }
    };

    ($id:ident, $name:expr, [$($stat:ident),*], $forbidden:ident, |$x:ident| $strat:expr) => {
        paste! {
            pub fn [<best_by_ $id:lower>]($x: PitcherRef) -> Option<f64> {
                Some($strat)
            }

            pub const $id: Algorithm = Algorithm {
                name: $name,
                forbidden: $forbidden,
                printed_stats: &[$(PrintedStat::$stat),*],
                strategy: [<best_by_ $id:lower>],
            };
        }
    };
}

algorithm!(SO9, "SO/9", [], Unforbidden, |x| x.stats.k_per_9);

algorithm!(RUTHLESSNESS, _, [SO9], Forbidden, |x| x.player.ruthlessness);

algorithm!(STAT_RATIO, "(SO/9)(SO/AB)", [SO9], Unforbidden, |x| {
    x.stats.k_per_9
        * (0.2
            + x.opponent
                .strikeouts(x.state)
                .zip(x.opponent.at_bats(x.state))
                .map(|(so, ab)| Some((so?, ab?)))
                .map(|x| x.map(|(so, ab)| so as f64 / ab as f64))
                .collect::<Option<Mean>>()?
                .mean())
});
