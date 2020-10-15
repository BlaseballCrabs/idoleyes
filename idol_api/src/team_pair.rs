use std::mem::take;

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

impl<T, E> Transpose<Result<TeamPair<T>, E>> for TeamPair<Result<T, E>> {
    fn transpose(self) -> Result<TeamPair<T>, E> {
        Ok(TeamPair {
            home: self.home?,
            away: self.away?,
        })
    }
}
