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

fn send_hook(urls: &[&str], data: &Event, retry: bool, test_mode: bool) {
    let content = match get_best(data) {
        Ok(content) => content,
        Err(err) => {
            warn!("Failed to get message: {}", err);
            if retry {
                debug!("Retrying...");
                send_hook(urls, data, false, test_mode);
                return;
            } else if test_mode {
                debug!("Sending test message");
                "Error getting best idols, ignoring due to test mode".into()
            } else {
                debug!("Not retrying");
                return;
            }
        }
    };
    info!("{}", content);
    debug!("Sending to {} webhooks", urls.len());
    for (i, url) in urls.iter().enumerate() {
        debug!("URL #{}", i + 1);
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
                        std::thread::sleep(std::time::Duration::from_millis(5000));
                        debug!("Reconnecting...");
                        *client = Client::new(url.clone());
                        continue;
                    }
                };
                break data;
            }
            Some(Err(err)) => {
                error!("Error receiving event: {}", err);
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Reconnecting...");
                *client = Client::new(url.clone());
                continue;
            }
            None => {
                warn!("Event stream ended");
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Reconnecting...");
                *client = Client::new(url.clone());
                continue;
            }
        }
    }
}

fn logger() -> Result<()> {
    let syslog_fmt = syslog::Formatter3164 {
        facility: syslog::Facility::LOG_USER,
        hostname: None,
        process: "idol_bot".to_owned(),
        pid: std::process::id() as _,
    };
    let (syslog, syslog_err): (fern::Dispatch, _) = match syslog::unix(syslog_fmt) {
        Ok(syslog) => (
            fern::Dispatch::new()
                .level(log::LevelFilter::Debug)
                .chain(syslog),
            None,
        ),
        Err(err) => (fern::Dispatch::new(), Some(err)),
    };
    fern::Dispatch::new()
        .level(log::LevelFilter::Warn)
        .level_for("idol_bot", log::LevelFilter::Trace)
        .level_for("idol_predictor", log::LevelFilter::Trace)
        .level_for("idol_api", log::LevelFilter::Trace)
        .chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    out.finish(format_args!(
                        "[{} {} {}] {}",
                        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                        record.level(),
                        record.target(),
                        message
                    ))
                })
                .chain(std::io::stdout()),
        )
        .chain(syslog)
        .apply()?;
    if let Some(err) = syslog_err {
        warn!("Error setting up syslog: {}", err);
    }
    Ok(())
}

fn main() -> Result<()> {
    logger()?;

    let urls_raw = dotenv::var("WEBHOOK_URL")?;
    let test_mode: usize = dotenv::var("TEST_MODE")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let urls: Vec<&str> = urls_raw.split(",").collect();
    let stream_url = Url::parse("https://www.blaseball.com/events/streamData")?;

    let mut client = Client::new(stream_url.clone());
    debug!("Connected");
    loop {
        let mut data = next_event(&mut client, &stream_url);
        debug!("Phase {}", data.value.games.sim.phase);
        if test_mode != 0 {
            info!("TESTING MODE");
            send_hook(&urls, &data, false, true);
            break;
        }
        match data.value.games.sim.phase {
            4 | 10 | 11 => {
                debug!("Postseason");
                if data.value.games.tomorrow_schedule.len() > 0 {
                    debug!("Betting allowed");
                    send_hook(&urls, &data, true, false);
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
                send_hook(&urls, &data, true, false);
                wait_for_next_regular_season_game();
            }
            _ => {
                debug!("Not season");
            }
        }
    }

    Ok(())
}
