use anyhow::Result;
use async_std::prelude::*;
use chrono::prelude::*;
use idol_api::models::Event;
use idol_api::State;
use idol_predictor::algorithms::{ALGORITHMS, JOKE_ALGORITHMS};
use log::*;
use rand::prelude::*;
use serde::Serialize;
use std::fmt::Write;
use std::pin::Pin;

#[derive(Debug, Serialize)]
pub struct Webhook<'a> {
    pub content: &'a str,
    pub avatar_url: &'static str,
}

async fn get_best(data: &Event) -> Result<String> {
    let day = data.value.games.sim.day;
    debug!("Building state");
    let state = State::from_event(data).await?;
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

async fn send_message(url: &str, content: &str) -> Result<()> {
    let hook = Webhook {
        content,
        avatar_url: "http://hs.hiveswap.com/ezodiac/images/aspect_7.png",
    };
    surf::post(url)
        .body(surf::Body::from_json(&hook).map_err(|x| x.into_inner())?)
        .send()
        .await
        .map_err(|x| x.into_inner())?;
    Ok(())
}

fn send_hook<'a>(
    urls: &'a [&'a str],
    data: &'a Event,
    retry: bool,
    test_mode: bool,
) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    Box::pin(async move {
        let content = match get_best(data).await {
            Ok(content) => content,
            Err(err) => {
                warn!("Failed to get message: {}", err);
                if retry {
                    debug!("Retrying...");
                    send_hook(urls, data, false, test_mode).await;
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
            match send_message(url, &content).await {
                Ok(_) => {
                    debug!("Sent");
                }
                Err(err) => {
                    warn!("Failed to send message: {}", err);
                    debug!("Retrying...");
                    match send_message(url, &content).await {
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
    })
}

type Client = async_sse::Decoder<surf::Response>;

async fn sse_connect(url: &str) -> Client {
    let mut surf_req = surf::Request::new(http_types::Method::Get, url.parse().unwrap());
    let http_req: &mut http_types::Request = surf_req.as_mut();
    async_sse::upgrade(http_req);

    let resp = match surf::client().send(surf_req.clone()).await {
        Ok(resp) => resp,
        Err(_) => {
            warn!("Failed to connect");
            std::thread::sleep(std::time::Duration::from_millis(5000));
            debug!("Retrying...");
            surf::client().send(surf_req).await.unwrap()
        }
    };

    async_sse::decode(resp)
}

async fn next_event(client: &mut Client, url: &str) -> Event {
    loop {
        debug!("Waiting for event");
        match client.next().await {
            Some(Ok(async_sse::Event::Message(message))) => {
                debug!("Received event");
                let data: Event = match serde_json::from_slice(&message.data()) {
                    Ok(data) => {
                        debug!("Parsed event");
                        data
                    }
                    Err(err) => {
                        error!("Couldn't parse event: {}", err);
                        std::thread::sleep(std::time::Duration::from_millis(5000));
                        debug!("Reconnecting...");
                        *client = sse_connect(url).await;
                        continue;
                    }
                };
                break data;
            }
            Some(Ok(async_sse::Event::Retry(retry))) => {
                debug!("got Retry: {:?}", retry);
                continue;
            }
            Some(Err(err)) => {
                error!("Error receiving event: {}", err);
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Reconnecting...");
                *client = sse_connect(url).await;
                continue;
            }
            None => {
                warn!("Event stream ended");
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Reconnecting...");
                *client = sse_connect(url).await;
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

#[async_std::main]
async fn main() -> Result<()> {
    logger()?;

    let urls_raw = dotenv::var("WEBHOOK_URL")?;
    let test_mode: usize = dotenv::var("TEST_MODE")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let urls: Vec<&str> = urls_raw.split(',').collect();
    let stream_url = "https://www.blaseball.com/events/streamData";

    let mut client = sse_connect(stream_url).await;
    debug!("Connected");
    loop {
        let mut data = next_event(&mut client, &stream_url).await;
        debug!("Phase {}", data.value.games.sim.phase);
        if test_mode != 0 {
            info!("TESTING MODE");
            send_hook(&urls, &data, false, true).await;
            break;
        }
        match data.value.games.sim.phase {
            4 | 10 | 11 | 13 | 14 => {
                debug!("Postseason");
                if !data.value.games.tomorrow_schedule.is_empty() {
                    debug!("Betting allowed");
                    send_hook(&urls, &data, true, false).await;
                } else {
                    debug!("No betting");
                }
                while !data.value.games.tomorrow_schedule.is_empty() {
                    debug!("Waiting for games to start...");
                    data = next_event(&mut client, &stream_url).await;
                }
                debug!("Games in progress");
            }
            2 => {
                debug!("Regular season");
                send_hook(&urls, &data, true, false).await;
                let day = data.value.games.sim.day;
                while data.value.games.sim.day == day {
                    debug!("Waiting for next day...");
                    data = next_event(&mut client, &stream_url).await;
                }
            }
            _ => {
                debug!("Not season");
            }
        }
    }

    Ok(())
}
