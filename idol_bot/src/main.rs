use anyhow::Result;
use chrono::prelude::*;
use eventsource::reqwest::Client;
use idol_api::models::Event;
use idol_api::State;
use idol_predictor::algorithms::{ALGORITHMS, JOKE_ALGORITHMS};
use log::*;
use rand::prelude::*;
use reqwest::Url;
use serde::Serialize;
use std::fmt::Write;

#[derive(Debug, Serialize)]
pub struct Webhook<'a> {
    pub content: &'a str,
}

fn get_best(data: &Event) -> Result<String> {
    let day = data.value.games.sim.day;
    debug!("Building state");
    let state = State::from_event(data)?;
    let mut text = String::new();
    writeln!(text, "**Day {}**", day + 2)?; // tomorrow, zero-indexed
    for algorithm in ALGORITHMS {
        debug!("{}", algorithm.name);
        algorithm.write_best_to(&state, &mut text)?;
    }
    let joke = JOKE_ALGORITHMS.choose(&mut thread_rng()).unwrap();
    debug!("Joke: {}", joke.name);
    joke.write_best_to(&state, &mut text)?;
    Ok(text)
}

fn send_hook(url: &str, data: &Event) -> Result<()> {
    let content = get_best(data)?;
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

fn wait_for_next_regular_season_game() {
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
    let default_filter = "idol_api,idol_predictor,idol_bot";
    let env = env_logger::Env::new().filter_or("RUST_LOG", default_filter);
    let mut builder = env_logger::Builder::from_env(env);
    builder.init();
}

fn main() -> Result<()> {
    logger();

    let url = dotenv::var("WEBHOOK_URL")?;

    let mut client =
        Client::new(Url::parse("https://www.blaseball.com/events/streamData").unwrap());
    debug!("Connected");
    loop {
        debug!("Waiting for event");
        let mut event = client.next().unwrap()?;
        debug!("Parsing");
        let mut data: Event = serde_json::from_str(&event.data)?;
        debug!("Phase {}", data.value.games.sim.phase);
        match data.value.games.sim.phase {
            4 | 10 | 11 => {
                debug!("Postseason");
                if data.value.games.tomorrow_schedule.len() > 0 {
                    debug!("Betting allowed");
                    send_hook(&url, &data)?;
                } else {
                    debug!("No betting");
                }
                while data.value.games.tomorrow_schedule.len() > 0 {
                    debug!("Waiting for games to start...");
                    event = client.next().unwrap()?;
                    data = serde_json::from_str(&event.data)?;
                }
                debug!("Games in progress");
            }
            2 => {
                debug!("Regular season");
                send_hook(&url, &data)?;
                wait_for_next_regular_season_game();
            }
            _ => {
                debug!("Not season");
            }
        }
    }
}
