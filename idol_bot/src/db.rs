use anyhow::Result;
use futures::prelude::*;
use idol_predictor::algorithms::{ALGORITHMS, JOKE_ALGORITHMS};
use log::*;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::collections::BTreeSet;

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

pub struct Webhook {
    pub id: i64,
    pub url: String,
}

pub struct AlgorithmRef {
    pub algorithm: i64,
    pub joke: bool,
}

impl Database {
    pub async fn connect(uri: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new().connect(uri).await?;

        let migrator = sqlx::migrate!("./migrations");
        migrator.run(&pool).await?;

        Ok(Self { pool })
    }

    pub fn webhooks(&self) -> impl Stream<Item = Result<Webhook>> + '_ {
        sqlx::query_as!(Webhook, "SELECT id, url FROM webhooks")
            .fetch(&self.pool)
            .err_into()
    }

    pub async fn try_algorithms(
        &self,
        webhook: &Webhook,
        joke: bool,
    ) -> Result<Option<BTreeSet<i64>>> {
        let db_algs = sqlx::query_scalar!(
            "SELECT algorithm FROM algorithms WHERE webhook_id = ? AND joke = ?",
            webhook.id,
            joke,
        )
        .fetch(&self.pool)
        .err_into::<anyhow::Error>()
        .try_collect::<BTreeSet<i64>>()
        .await?;

        if db_algs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(db_algs))
        }
    }

    pub async fn algorithms(&self, webhook: &Webhook, joke: bool) -> Result<BTreeSet<i64>> {
        if let Some(algs) = self.try_algorithms(webhook, joke).await? {
            Ok(algs)
        } else {
            let algs = if joke { JOKE_ALGORITHMS } else { ALGORITHMS };

            if let Some(other_algs) = self.try_algorithms(webhook, !joke).await? {
                Ok(algs
                    .iter()
                    .copied()
                    .filter(|x| !other_algs.contains(x))
                    .collect())
            } else {
                Ok(algs.iter().copied().collect())
            }
        }
    }

    pub async fn count(&self) -> Result<i32> {
        Ok(sqlx::query!("SELECT COUNT(*) as count FROM webhooks")
            .fetch_one(&self.pool)
            .await?
            .count)
    }

    pub async fn add_urls(&self, urls: impl Iterator<Item = &str>) -> Result<()> {
        let mut transaction = self.pool.begin().await?;
        for url in urls {
            debug!("adding URL: {:?}", url);
            sqlx::query!("INSERT OR IGNORE INTO webhooks (url) VALUES (?)", url)
                .execute(&mut transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn add_url(&self, url: &str) -> Result<()> {
        self.add_urls(std::iter::once(url)).await?;
        Ok(())
    }

    pub async fn remove_url(&self, url: &str) -> Result<()> {
        sqlx::query!("DELETE FROM webhooks WHERE url = ?", url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
