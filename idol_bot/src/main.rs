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

    loop {
        let mut data = client.next_event().await?;
        debug!("Phase {}", data.value.games.sim.phase);
        if test_mode != 0 {
            info!("TESTING MODE");
            send_hook(&db, &data, false, true).await?;
            break;
        }
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
                    data = client.next_event().await?;
                }
                debug!("Games in progress");
            }
            2 => {
                debug!("Regular season");
                send_hook(&db, &data, true, false).await?;
                let day = data.value.games.sim.day;
                while data.value.games.sim.day == day {
                    debug!("Waiting for next day...");
                    data = client.next_event().await?;
                }
            }
            _ => {
                debug!("Not season");
            }
        }
    }

    Ok(())
}
