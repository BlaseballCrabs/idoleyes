use super::{Algorithm, Forbidden::*, PitcherRef, PrintedStat};
use average::Mean;

pub fn best_by_so9(x: PitcherRef) -> Option<f64> {
    Some(x.stats.k_per_9)
}

pub const SO9: Algorithm = Algorithm {
    name: "SO/9",
    forbidden: Unforbidden,
    printed_stats: &[],
    strategy: best_by_so9,
};

pub fn best_by_ruthlessness(x: PitcherRef) -> Option<f64> {
    Some(x.player.ruthlessness)
}

pub const RUTHLESSNESS: Algorithm = Algorithm {
    name: "ruthlessness",
    forbidden: Forbidden,
    printed_stats: &[PrintedStat::SO9],
    strategy: best_by_ruthlessness,
};

pub fn best_by_stat_ratio(x: PitcherRef) -> Option<f64> {
    Some(
        x.stats.k_per_9
            * x.opponent
                .strikeouts(x.state)
                .zip(x.opponent.at_bats(x.state))
                .map(|(so, ab)| Some((so?, ab?)))
                .map(|x| x.map(|(so, ab)| so as f64 / ab as f64))
                .collect::<Option<Mean>>()?
                .mean(),
    )
}

pub const STAT_RATIO: Algorithm = Algorithm {
    name: "(SO/9)(SO/AB)",
    forbidden: Unforbidden,
    printed_stats: &[PrintedStat::SO9],
    strategy: best_by_stat_ratio,
};
