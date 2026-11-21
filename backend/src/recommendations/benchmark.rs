use std::{sync::Arc, time::Instant};

use anyhow::Context;
use sqlx::{Connection, MySqlConnection, MySqlPool, mysql::MySqlConnectOptions};

use super::{Filters, cache, ranked_cards};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a local seeded MariaDB with application migrations"]
async fn compare_hydrated_ranking_latency() -> anyhow::Result<()> {
    let url = std::env::var("RECOMMENDATIONS_BENCH_DATABASE_URL")
        .context("set RECOMMENDATIONS_BENCH_DATABASE_URL to a seeded local MariaDB")?;
    let options: MySqlConnectOptions = url.parse()?;
    anyhow::ensure!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]"
    ));
    let pool = MySqlPool::connect_with(options.clone()).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(&pool)
        .await?;
    let (first, second) = tokio::try_join!(
        compare_replica(pool.clone(), options.clone()),
        compare_replica(pool, options),
    )?;
    let mut stoolap = [first.0, second.0].concat();
    let mut mariadb = [first.1, second.1].concat();
    stoolap.sort_unstable_by(f64::total_cmp);
    mariadb.sort_unstable_by(f64::total_cmp);
    let p95 = |sample: &[f64]| sample[(sample.len() * 95).div_ceil(100) - 1];
    println!(
        "{count} catalog products, two private caches, {} paired samples including refresh periods, p95 Stoolap {:.3} ms / MariaDB {:.3} ms",
        stoolap.len(),
        p95(&stoolap),
        p95(&mariadb),
    );
    Ok(())
}

async fn compare_replica(
    pool: MySqlPool,
    options: MySqlConnectOptions,
) -> anyhow::Result<(Vec<f64>, Vec<f64>)> {
    let cache = Arc::new(cache::Cache::default());
    let started = Instant::now();
    cache
        .refresh(cache::read_catalog(&pool).await?, started)
        .await?;
    let filters = Filters::default();
    let key = "benchmark";
    let expected = cache
        .rank(
            key.into(),
            filters.clone(),
            Vec::new(),
            time::OffsetDateTime::now_utc().unix_timestamp(),
            Vec::new(),
        )
        .await?
        .context("benchmark cache unavailable")?;
    anyhow::ensure!(!expected.is_empty(), "seed the benchmark catalog first");
    let mut source = MySqlConnection::connect_with(&options).await?;
    sqlx::query("CREATE TEMPORARY TABLE recommendation_benchmark (cache_key VARCHAR(64) PRIMARY KEY, ids JSON NOT NULL)")
        .execute(&mut source).await?;
    sqlx::query("INSERT INTO recommendation_benchmark(cache_key,ids) VALUES (?,?)")
        .bind(key)
        .bind(sqlx::types::Json(&expected))
        .execute(&mut source)
        .await?;
    let mut cache_timings = Vec::new();
    let mut database_timings = Vec::new();
    for _ in 0..4 {
        let refresh_cache = Arc::clone(&cache);
        let refresh_pool = pool.clone();
        let refresh = tokio::spawn(async move {
            let started = Instant::now();
            refresh_cache
                .refresh(cache::read_catalog(&refresh_pool).await?, started)
                .await
        });
        for sample in 0..40 {
            let mut cards = Vec::new();
            for stoolap in [sample % 2 == 0, sample % 2 != 0] {
                let started = Instant::now();
                let mut ids = None;
                if stoolap && let cache::Lookup::Hit(ranking) = cache.lookup(key.into()).await? {
                    ids = Some(ranking);
                }
                let ids = match ids {
                    Some(ids) => ids,
                    None => {
                        sqlx::query_scalar::<_, sqlx::types::Json<Vec<i64>>>(
                            "SELECT ids FROM recommendation_benchmark WHERE cache_key=?",
                        )
                        .bind(key)
                        .fetch_one(&mut source)
                        .await?
                        .0
                    }
                };
                let mut items = ranked_cards(&pool, &filters, &ids, 8)
                    .await
                    .map_err(|error| anyhow::anyhow!("{}", error.1))?;
                items.truncate(8);
                cards.push(items);
                let elapsed = started.elapsed().as_secs_f64() * 1_000.0;
                if stoolap {
                    cache_timings.push(elapsed);
                } else {
                    database_timings.push(elapsed);
                }
            }
            anyhow::ensure!(
                cards[0] == cards[1],
                "benchmark hydration changed between paths"
            );
        }
        refresh.await??;
    }
    Ok((cache_timings, database_timings))
}
