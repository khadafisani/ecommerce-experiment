use std::path::Path;

use anyhow::Context;
use sqlx::{MySql, MySqlPool, Transaction};

use crate::{App, auth, catalog, search};

const BASE_BATCH: &str = "market-fixtures-v1";
const HISTORY_BATCH: &str = "market-history-10000-v1";
const PASSWORD: &str = "ToscaTest2026!";

pub async fn run(app: &App) -> anyhow::Result<()> {
    if insert_rows(&app.db, app.tax).await? {
        tracing::info!(
            "Inserted 24 shops, 768 products, 2,304 variants and 1,800 baseline checkouts"
        );
    }
    if insert_history(&app.db, app.tax).await? {
        tracing::info!("Added 8,200 dated checkouts for a total of 10,000 fixture checkouts");
    } else {
        tracing::info!("The 10,000-checkout fixture batch already exists");
    }
    if let Err(error) = search::rebuild(app).await {
        tracing::warn!(%error, "Fixtures are in MariaDB. Search will sync the next time the API starts.");
    }
    Ok(())
}

async fn insert_rows(db: &MySqlPool, tax: i64) -> anyhow::Result<bool> {
    let mut tx = db.begin().await?;
    if !claim_batch(&mut tx, BASE_BATCH).await? {
        return Ok(false);
    }

    let hash = auth::hash(PASSWORD.to_owned())
        .await
        .map_err(|error| anyhow::anyhow!(error.1))?;
    sqlx::query("SET @seed_password = ?, @seed_tax = ?, @seed_now = UTC_TIMESTAMP()")
        .bind(hash)
        .bind(tax)
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql(include_str!("../fixtures/test-market.sql"))
        .execute(&mut *tx)
        .await
        .context("Inserting test-market fixtures")?;

    let paths: Vec<String> = sqlx::query_scalar(
        "SELECT JSON_UNQUOTE(JSON_EXTRACT(r.media,'$[0]')) FROM returns r JOIN fixture_order_id f ON f.order_id=r.order_id",
    )
    .fetch_all(&mut *tx)
    .await?;
    write_return_evidence(&paths).await?;
    sqlx::query("DROP TEMPORARY TABLE fixture_order_id")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

async fn claim_batch(tx: &mut Transaction<'_, MySql>, batch: &str) -> anyhow::Result<bool> {
    let claimed = sqlx::query("INSERT INTO test_seed_batches(name) VALUES (?)")
        .bind(batch)
        .execute(&mut **tx)
        .await;
    match claimed {
        Ok(_) => Ok(true),
        Err(sqlx::Error::Database(error)) if error.is_unique_violation() => Ok(false),
        Err(error) => Err(error.into()),
    }
}

async fn insert_history(db: &MySqlPool, tax: i64) -> anyhow::Result<bool> {
    let mut tx = db.begin().await?;
    if !claim_batch(&mut tx, HISTORY_BATCH).await? {
        return Ok(false);
    }
    sqlx::query("SET @seed_tax = ?, @seed_now = UTC_TIMESTAMP()")
        .bind(tax)
        .execute(&mut *tx)
        .await?;
    sqlx::raw_sql(include_str!("../fixtures/test-market-history.sql"))
        .execute(&mut *tx)
        .await
        .context("Adding dated fixture checkouts")?;
    let inserted: i64 = sqlx::query_scalar("SELECT CAST(@history_inserted AS SIGNED)")
        .fetch_one(&mut *tx)
        .await?;
    anyhow::ensure!(
        inserted == 8_200,
        "Expected 8,200 dated fixture checkouts, inserted {inserted}"
    );
    tx.commit().await?;
    Ok(true)
}

async fn write_return_evidence(paths: &[String]) -> anyhow::Result<()> {
    let avif = tokio::task::spawn_blocking(|| {
        img4avif::Converter::new(img4avif::Config::default())?
            .convert(include_bytes!("../../frontend/public/catalog/mug.jpg"))
    })
    .await??;
    let folder = Path::new("private-uploads");
    tokio::fs::create_dir_all(folder).await?;
    for path in paths {
        let name = path
            .strip_prefix(catalog::RETURN_MEDIA_PREFIX)
            .context("Unexpected fixture attachment URL")?;
        anyhow::ensure!(
            catalog::return_media_name(name).is_some(),
            "Invalid fixture attachment filename"
        );
        tokio::fs::write(folder.join(name), &avif).await?;
    }
    Ok(())
}
