use anyhow::Result;
use async_std::prelude::*;
use async_std::task;
use idol_bot::{
    db::Database,
    events::Client,
    logger,
    oauth_listener::{self, OAuth},
    send_hook,
};
use log::*;

#[async_std::main]
async fn main() -> Result<()> {
    logger::init()?;

    let test_mode: Option<usize> = dotenv::var("TEST_MODE").ok().and_then(|x| x.parse().ok());
    let stream_url = "https://www.blaseball.com/events/streamData";

    let db_uri = dotenv::var("DATABASE_URL")?;

    let db = Database::connect(&db_uri).await?;
    debug!("Connected to database");

    let redirect_uri = dotenv::var("REDIRECT_URI")?;
    let client_id = dotenv::var("CLIENT_ID")?;
    let client_secret = dotenv::var("CLIENT_SECRET")?;

    let manual_webhook_urls = dotenv::var("WEBHOOK_URL");
    db.add_urls(
        manual_webhook_urls
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty()),
    )
    .await?;

    let mut client = Client::connect(stream_url).await?;
    debug!("Connected to Blaseball");

    if let Some(test_mode) = test_mode {
        let data = client.next_event().await?;
        debug!("Phase {}", data.value.games.sim.phase);
        info!("TESTING MODE");
        send_hook(&db, &data, false, Some(test_mode)).await?;
    } else {
        let bot = task::spawn(client.run(&db));
        let listener = task::spawn(oauth_listener::listen(
            &db,
            OAuth {
                redirect_uri,
                client_id,
                client_secret,
            },
        ));
        bot.race(listener).await?;
    }

    Ok(())
}
