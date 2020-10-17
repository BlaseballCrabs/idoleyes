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
        match algorithm.write_best_to(&state, &mut text) {
            Ok(_) => {
                debug!("Succeeded");
            }
            Err(err) => {
                warn!("Algorithm failed: {}", err);
            }
        }
    }
    loop {
        let joke = JOKE_ALGORITHMS.choose(&mut thread_rng()).unwrap();
        debug!("Joke: {}", joke.name);
        match joke.write_best_to(&state, &mut text) {
            Ok(_) => {
                debug!("Succeeded");
                break;
            }
            Err(err) => {
                warn!("Joke algorithm failed: {}", err);
            }
        }
    }
    Ok(text)
}

fn send_message(url: &str, content: &str) -> Result<()> {
    let hook = Webhook { content };
    reqwest::blocking::Client::new()
        .post(url)
        .json(&hook)
        .send()?;
    Ok(())
}

fn send_hook(url: &str, data: &Event, retry: bool) {
    let content = match get_best(data) {
        Ok(content) => content,
        Err(err) => {
            warn!("Failed to get message: {}", err);
            if retry {
                debug!("Retrying...");
                send_hook(url, data, false);
            } else {
                debug!("Not retrying");
            }
            return;
        }
    };
    info!("{}", content);
    debug!("Sending");
    match send_message(url, &content) {
        Ok(_) => {
            debug!("Sent");
        }
        Err(err) => {
            warn!("Failed to send message: {}", err);
            debug!("Retrying...");
            match send_message(url, &content) {
                Ok(_) => {
                    debug!("Sent");
                }
                Err(err) => {
                    error!("Failed to send twice, not retrying: {}", err);
                }
            }
        }
    }
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

fn next_event(client: &mut Client, url: &Url) -> Event {
    loop {
        debug!("Waiting for event");
        match client.next() {
            Some(Ok(event)) => {
                debug!("Received event");
                let data: Event = match serde_json::from_str(&event.data) {
                    Ok(data) => {
                        debug!("Parsed event");
                        data
                    }
                    Err(err) => {
                        error!("Couldn't parse event: {}", err);
                        debug!("Reconnecting...");
                        *client = Client::new(url.clone());
                        continue;
                    }
                };
                break data;
            }
            Some(Err(err)) => {
                error!("Error receiving event: {}", err);
                debug!("Reconnecting...");
                *client = Client::new(url.clone());
                continue;
            }
            None => {
                warn!("Event stream ended");
                debug!("Reconnecting...");
                *client = Client::new(url.clone());
                continue;
            }
        }
    }
}

fn main() -> Result<()> {
    logger();

    let url = dotenv::var("WEBHOOK_URL")?;
    let stream_url = Url::parse("https://www.blaseball.com/events/streamData")?;

    let mut client = Client::new(stream_url.clone());
    debug!("Connected");
    loop {
        let mut data = next_event(&mut client, &stream_url);
        debug!("Phase {}", data.value.games.sim.phase);
        match data.value.games.sim.phase {
            4 | 10 | 11 => {
                debug!("Postseason");
                if data.value.games.tomorrow_schedule.len() > 0 {
                    debug!("Betting allowed");
                    send_hook(&url, &data, true);
                } else {
                    debug!("No betting");
                }
                while data.value.games.tomorrow_schedule.len() > 0 {
                    debug!("Waiting for games to start...");
                    data = next_event(&mut client, &stream_url);
                }
                debug!("Games in progress");
            }
            2 => {
                debug!("Regular season");
                send_hook(&url, &data, true);
                wait_for_next_regular_season_game();
            }
            _ => {
                debug!("Not season");
            }
        }
    }
}
