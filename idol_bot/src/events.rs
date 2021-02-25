use anyhow::Result;
use async_std::prelude::*;
use idol_api::models::Event;
use log::*;

type Client = async_sse::Decoder<surf::Response>;

pub async fn sse_connect(url: &str) -> Result<Client> {
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

    Ok(async_sse::decode(resp))
}

pub async fn next_event(client: &mut Client, url: &str) -> Result<Event> {
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
                        *client = sse_connect(url).await?;
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
                *client = sse_connect(url).await?;
                continue;
            }
            None => {
                warn!("Event stream ended");
                std::thread::sleep(std::time::Duration::from_millis(5000));
                debug!("Reconnecting...");
                *client = sse_connect(url).await?;
                continue;
            }
        }
    }
}
