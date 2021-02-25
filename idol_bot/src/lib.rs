use anyhow::{bail, Result};
use async_std::prelude::*;
use chrono::prelude::*;
use db::Database;
use idol_api::models::Event;
use idol_api::State;
use idol_predictor::algorithms::{ALGORITHMS, JOKE_ALGORITHMS};
use log::*;
use rand::prelude::*;
use serde::Serialize;
use std::fmt::Write;
use std::pin::Pin;

pub mod db;
pub mod events;

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

pub fn send_hook<'a>(
    db: &'a Database,
    data: &'a Event,
    retry: bool,
    test_mode: bool,
) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>> {
    Box::pin(async move {
        let content = match get_best(data).await {
            Ok(content) => content,
            Err(err) => {
                warn!("Failed to get message: {}", err);
                if retry {
                    debug!("Retrying...");
                    return send_hook(db, data, false, test_mode).await;
                } else if test_mode {
                    debug!("Sending test message");
                    "Error getting best idols, ignoring due to test mode".into()
                } else {
                    debug!("Not retrying");
                    bail!("Failed to get message: {}", err);
                }
            }
        };
        info!("{}", content);
        debug!("Sending to {} webhooks", db.count().await?);
        let mut urls = db.urls().enumerate();
        while let Some((i, url)) = urls.next().await {
            let url = url?;

            debug!("URL #{}", i + 1);
            match send_message(&url, &content).await {
                Ok(_) => {
                    debug!("Sent");
                }
                Err(err) => {
                    warn!("Failed to send message: {}", err);
                    debug!("Retrying...");
                    match send_message(&url, &content).await {
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
        Ok(())
    })
}

pub fn logger() -> Result<()> {
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
