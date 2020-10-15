use anyhow::{anyhow, Result};
use chrono::prelude::*;
use idol_api::models::Player;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerUpdate {
    pub data: Player,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlayerUpdates {
    pub data: Vec<PlayerUpdate>,
}

pub fn player_at(id: &str, time: DateTime<Utc>) -> Result<Player> {
    let timestamp = time.to_rfc3339_opts(SecondsFormat::Secs, true);
    let client = reqwest::blocking::Client::new();
    let mut updates: PlayerUpdates = client
        .get("https://api.sibr.dev/chronicler/v1/players/updates?order=desc&count=1")
        .query(&[("after", timestamp)])
        .query(&[("player", id)])
        .send()?
        .json()?;
    let update = updates
        .data
        .pop()
        .ok_or_else(|| anyhow!("No updates for player {} at {}!", id, time))?;
    Ok(update.data)
}
