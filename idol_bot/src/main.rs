use anyhow::{anyhow, Result};
use chrono::prelude::*;
use eventsource::reqwest::Client;
use idol_predictor::{algorithms, Event, State};
use log::*;
use reqwest::Url;
use serde::Serialize;
use std::fmt::Write;

#[derive(Debug, Serialize)]
pub struct Webhook<'a> {
    pub content: &'a str,
}

fn get_best() -> Result<Option<String>> {
    let mut client =
        Client::new(Url::parse("https://www.blaseball.com/events/streamData").unwrap());
    debug!("Connected, waiting for event");
    let event = client
        .next()
        .ok_or_else(|| anyhow!("Didn't get event!"))??;
    drop(client);
    debug!("Parsing");
    let data: Event = serde_json::from_str(&event.data)?;
    debug!("Phase {}", data.value.games.sim.phase);
    if data.value.games.sim.phase != 2 {
        debug!("Not regular season");
        return Ok(None);
    }
    let day = data.value.games.sim.day;
    debug!("Building state");
    let state = State::from_event(data)?;
    let mut text = String::new();
    writeln!(text, "**Day {}**", day + 2)?; // tomorrow, zero-indexed
    debug!("SO/9");
    algorithms::SO9.write_best_to(&state, &mut text)?;
    debug!("Ruthlessness");
    algorithms::RUTHLESSNESS.write_best_to(&state, &mut text)?;
    debug!("(SO/9)(SO/AB)");
    algorithms::STAT_RATIO.write_best_to(&state, &mut text)?;
    Ok(Some(text))
}

fn send_hook(url: &str) -> Result<()> {
    let content = match get_best()? {
        Some(x) => x,
        None => return Ok(()),
    };
    info!("{}", content);
    debug!("Sending");
    let hook = Webhook { content: &content };
    reqwest::blocking::Client::new()
        .post(url)
        .json(&hook)
        .send()?;
    debug!("Sent");
    Ok(())
}

fn wait_for_next_game() {
    let now = Utc::now();
    let one_hour = chrono::Duration::hours(1);
    let later = now + one_hour;
    let game = later
        .with_minute(1)
        .unwrap()
        .with_second(0)
        .unwrap()
        .with_nanosecond(0)
        .unwrap();
    let time_till_game = game - now;
    debug!("Sleeping for {} (until {})", time_till_game, game);
    std::thread::sleep(time_till_game.to_std().unwrap());
}

fn logger() {
    let default_filter = "idol_predictor,idol_bot";
    let env = env_logger::Env::new().filter_or("RUST_LOG", default_filter);
    let mut builder = env_logger::Builder::from_env(env);
    builder.init();
}

fn main() -> Result<()> {
    logger();

    let url = dotenv::var("WEBHOOK_URL")?;

    loop {
        send_hook(&url)?;
        wait_for_next_game();
    }
}
