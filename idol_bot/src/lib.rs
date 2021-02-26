use anyhow::{ensure, Result};
use db::Database;
use futures::prelude::*;
use idol_api::models::Event;
use idol_api::State;
use idol_predictor::algorithms::ALL_ALGORITHMS;
use log::*;
use rand::prelude::*;
use serde::Serialize;
use std::fmt::Write;
use std::pin::Pin;
use std::sync::Arc;

pub mod db;
pub mod events;
pub mod logger;
pub mod oauth_listener;

#[derive(Debug, Serialize)]
pub struct WebhookPayload<'a> {
    pub content: &'a str,
    pub avatar_url: &'static str,
}

async fn get_best(data: &Event) -> Result<Vec<Option<String>>> {
    debug!("Building state");
    let state = State::from_event(data).await?;

    Ok(ALL_ALGORITHMS
        .iter()
        .map(|algorithm| {
            debug!("{}", algorithm.name);
            let mut text = String::new();
            match algorithm.write_best_to(&state, &mut text) {
                Ok(_) => {
                    debug!("Succeeded");
                    Some(text)
                }
                Err(err) => {
                    warn!("Algorithm failed: {}", err);
                    None
                }
            }
        })
        .collect())
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
        let day = data.value.games.sim.day + 2;

        let best: Arc<[_]> = match get_best(data).await {
            Ok(content) => Arc::from(content),
            Err(err) => {
                warn!("Failed to get best: {}", err);
                if retry {
                    debug!("Retrying...");
                    return send_hook(db, data, false, test_mode).await;
                } else if test_mode {
                    debug!("Sending test message");
                    Arc::from(vec![Some(
                        "Error getting best idols, ignoring due to test mode".into(),
                    )])
                } else {
                    debug!("Not retrying");
                    return Err(err.context("Failed to get best"));
                }
            }
        };

        debug!("Sending to {} webhooks", db.count().await?);
        db.webhooks()
            .enumerate()
            .map(|(i, x)| Ok::<_, anyhow::Error>((i, x?)))
            .try_for_each_concurrent(None, |(i, webhook)| {
                let best = best.clone();
                async move {
                    debug!("URL #{}", i + 1);

                    let mut content = String::new();

                    writeln!(content, "**Day {}**", day)?;

                    let algorithms = db.algorithms(&webhook, false).await?;

                    for algorithm in algorithms {
                        if let Some(Some(best)) = best.get(algorithm as usize) {
                            write!(content, "{}", best)?;
                        }
                    }

                    let joke_algorithm_ids = db.algorithms(&webhook, true).await?;
                    let joke_algorithms = joke_algorithm_ids
                        .iter()
                        .filter_map(|&x| best.get(x as usize).map(Option::as_ref).flatten());
                    if let Some(joke_algorithm) = joke_algorithms.choose(&mut thread_rng()) {
                        write!(content, "{}", joke_algorithm)?;
                    }

                    match send_message(&db, &webhook.url, &content).await {
                        Ok(_) => {
                            debug!("Sent");
                        }
                        Err(err) => {
                            warn!("Failed to send message: {}", err);
                            debug!("Retrying...");
                            match send_message(&db, &webhook.url, &content).await {
                                Ok(_) => {
                                    debug!("Sent");
                                }
                                Err(err) => {
                                    error!("Failed to send twice, not retrying: {}", err);
                                }
                            }
                        }
                    }

                    Ok(())
                }
            })
            .await?;
        Ok(())
    })
}
