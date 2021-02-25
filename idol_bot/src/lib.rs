use anyhow::{bail, ensure, Result};
use async_std::prelude::*;
use db::{Database, Webhook};
use idol_api::models::Event;
use idol_api::State;
use idol_predictor::algorithms::{ALGORITHMS, JOKE_ALGORITHMS, LIFT};
use log::*;
use rand::prelude::*;
use serde::Serialize;
use std::fmt::Write;
use std::pin::Pin;

pub mod db;
pub mod events;
pub mod logger;
pub mod oauth_listener;

#[derive(Debug, Serialize)]
pub struct WebhookPayload<'a> {
    pub content: &'a str,
    pub avatar_url: &'static str,
}

async fn get_best(data: &Event, liftcord: bool) -> Result<String> {
    let lift_joke = if liftcord { None } else { Some(&LIFT) };
    let lift_always = if liftcord { Some(&LIFT) } else { None };

    let day = data.value.games.sim.day;
    debug!("Building state");
    let state = State::from_event(data).await?;
    let mut text = String::new();
    writeln!(text, "**Day {}**", day + 2)?; // tomorrow, zero-indexed
    for algorithm in ALGORITHMS.iter().chain(lift_always) {
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

    let joke_algorithms = JOKE_ALGORITHMS.iter().chain(lift_joke);
    loop {
        let joke = joke_algorithms.clone().choose(&mut thread_rng()).unwrap();

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

async fn send_message(db: &Database, url: &str, content: &str) -> Result<()> {
    let hook = WebhookPayload {
        content,
        avatar_url: "http://hs.hiveswap.com/ezodiac/images/aspect_7.png",
    };
    let status = surf::post(url)
        .body(surf::Body::from_json(&hook).map_err(|x| x.into_inner())?)
        .send()
        .await
        .map_err(|x| x.into_inner())?
        .status();

    if status == surf::StatusCode::NotFound {
        debug!("webhook removed, deleting from database");
        db.remove_url(url).await?;
    } else {
        ensure!(status.is_success(), "Couldn't send webhook: {}", status);
    }

    Ok(())
}

pub fn send_hook<'a>(
    db: &'a Database,
    data: &'a Event,
    retry: bool,
    test_mode: bool,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        macro_rules! try_get_best {
            ($liftcord:expr) => {
                match get_best(data, $liftcord).await {
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
                }
            };
        };

        let content = try_get_best!(false);
        let liftcord_content = try_get_best!(true);

        info!("{}", content);
        debug!("Sending to {} webhooks", db.count().await?);
        let mut webhooks = db.webhooks().enumerate();
        while let Some((i, webhook)) = webhooks.next().await {
            let Webhook { url, liftcord } = webhook?;

            let content = if liftcord {
                &liftcord_content
            } else {
                &content
            };

            debug!("URL #{}", i + 1);
            match send_message(&db, &url, &content).await {
                Ok(_) => {
                    debug!("Sent");
                }
                Err(err) => {
                    warn!("Failed to send message: {}", err);
                    debug!("Retrying...");
                    match send_message(&db, &url, &content).await {
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
