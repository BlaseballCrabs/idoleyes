use anyhow::Result;
use async_std::prelude::*;
use idol_api::models::Event;
use log::*;

pub struct Client {
    url: String,
    decoder: async_sse::Decoder<surf::Response>,
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
        })
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
                            debug!("Reconnecting...");
                            *self = Self::connect(&self.url).await?;
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
                    debug!("Reconnecting...");
                    *self = Self::connect(&self.url).await?;
                    continue;
                }
                None => {
                    warn!("Event stream ended");
                    std::thread::sleep(std::time::Duration::from_millis(5000));
                    debug!("Reconnecting...");
                    *self = Self::connect(&self.url).await?;
                    continue;
                }
            }
        }
    }
}
