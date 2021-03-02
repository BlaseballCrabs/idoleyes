use super::{db::Database, send_hook};
use anyhow::Result;
use async_std::prelude::*;
use idol_api::models::Event;
use log::*;
use std::time::{Duration, Instant};

pub struct Client {
    url: String,
    decoder: async_sse::Decoder<surf::Response>,
    opened_at: Instant,
}

impl Client {
    pub async fn connect(url: &str) -> Result<Self> {
        let mut surf_req = surf::Request::new(http_types::Method::Get, surf::Url::parse(url)?);
        let http_req: &mut http_types::Request = surf_req.as_mut();
        async_sse::upgrade(http_req);

        let resp = match surf::client().send(surf_req.clone()).await {
            Ok(resp) => resp,
            Err(_) => {
                warn!("Failed to connect");
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Retrying...");
                surf::client()
                    .send(surf_req)
                    .await
                    .map_err(|x| x.into_inner())?
            }
        };

        let decoder = async_sse::decode(resp);

        Ok(Self {
            decoder,
            url: url.to_string(),
            opened_at: Instant::now(),
        })
    }

    async fn reconnect(&mut self) -> Result<()> {
        debug!("Reconnecting...");
        *self = Self::connect(&self.url).await?;
        Ok(())
    }

    pub async fn next_event(&mut self) -> Result<Event> {
        loop {
            debug!("Waiting for event");
            match self.decoder.next().await {
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
                            self.reconnect().await?;
                            continue;
                        }
                    };
                    break Ok(data);
                }
                Some(Ok(async_sse::Event::Retry(retry))) => {
                    debug!("got Retry: {:?}", retry);
                    continue;
                }
                Some(Err(err)) => {
                    error!("Error receiving event: {}", err);
                    std::thread::sleep(std::time::Duration::from_millis(5000));
                    self.reconnect().await?;
                    continue;
                }
                None => {
                    let elapsed = self.opened_at.elapsed();
                    if elapsed < Duration::from_secs(30) {
                        error!("Event stream ended in {:?}", elapsed);
                        std::thread::sleep(std::time::Duration::from_millis(10000));
                    } else if elapsed <= Duration::from_secs(44) {
                        warn!("Event stream ended in {:?}", elapsed);
                        std::thread::sleep(std::time::Duration::from_millis(5000));
                    } else {
                        debug!("Event stream ended in {:?}", elapsed);
                    }
                    self.reconnect().await?;
                    continue;
                }
            }
        }
    }

    pub fn run(mut self, db: &Database) -> impl Future<Output = Result<()>> {
        let db = db.clone();
        async move {
            loop {
                let mut data = self.next_event().await?;
                debug!("Phase {}", data.value.games.sim.phase);
                match data.value.games.sim.phase {
                    4 | 10 | 11 | 13 | 14 => {
                        debug!("Postseason");
                        if !data.value.games.tomorrow_schedule.is_empty() {
                            debug!("Betting allowed");
                            send_hook(&db, &data, true, false).await?;
                        } else {
                            debug!("No betting");
                        }
                        while !data.value.games.tomorrow_schedule.is_empty() {
                            debug!("Waiting for games to start...");
                            data = self.next_event().await?;
                        }
                        debug!("Games in progress");
                    }
                    2 => {
                        debug!("Regular season");
                        send_hook(&db, &data, true, false).await?;
                        let day = data.value.games.sim.day;
                        while data.value.games.sim.day == day {
                            debug!("Waiting for next day...");
                            data = self.next_event().await?;
                        }
                    }
                    _ => {
                        debug!("Not season");
                    }
                }
            }
        }
    }
}
