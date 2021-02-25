use anyhow::Result;
use idol_bot::{db::Database, events::Client, logger, send_hook};
use log::*;

#[async_std::main]
async fn main() -> Result<()> {
    logger::init()?;

    let test_mode: usize = dotenv::var("TEST_MODE")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(0);
    let stream_url = "https://www.blaseball.com/events/streamData";

    let db_uri = dotenv::var("DATABASE_URL")?;

    let db = Database::connect(&db_uri).await?;
    debug!("Connected to database");

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

    if test_mode != 0 {
        let data = client.next_event().await?;
        debug!("Phase {}", data.value.games.sim.phase);
        info!("TESTING MODE");
        send_hook(&db, &data, false, true).await?;
    } else {
        client.run(&db).await?;
    }

    Ok(())
}
