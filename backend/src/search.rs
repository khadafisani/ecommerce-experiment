use std::{collections::HashMap, time::Duration};

use anyhow::Context;
use axum::extract::{Query, State};
use meilisearch_sdk::{
    client::Client,
    errors::{Error as MeiliError, ErrorCode},
    indexes::Index,
    search::{MatchingStrategies, Selectors},
    settings::Settings,
    task_info::TaskInfo,
    tasks::Task,
};
use serde::Deserialize;
use sqlx::{Connection, MySqlConnection, MySqlPool, QueryBuilder};

use crate::{App, Error, Json, MySql, Result, StatusCode, Value, embeddings, json, setting, text};

mod products;
mod shops;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const INDEX_CHECK_INTERVAL: Duration = Duration::from_secs(15);
const STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const TASK_TIMEOUT: Duration = Duration::from_secs(15);
const BULK_TASK_TIMEOUT: Duration = Duration::from_secs(600);
const LIVE_BATCH_SIZE: u32 = 256;
const BULK_BATCH_SIZE: u32 = 50_000;
const SQL_BATCH_SIZE: usize = 10_000;

#[derive(Clone)]
pub struct Search {
    client: Client,
    products: Index,
    shops: Index,
    hybrid: bool,
    embedder: embeddings::Embedder,
}

impl Search {
    pub fn new(hybrid: bool, embedder: embeddings::Embedder) -> anyhow::Result<Self> {
        let suffix = setting("MEILI_INDEX_SUFFIX", "");
        let client = Client::new(
            setting("MEILI_URL", "http://127.0.0.1:7700"),
            std::env::var("MEILI_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
        )?;
        Ok(Self {
            products: client.index(format!("{}{suffix}", products::INDEX.uid_prefix)),
            shops: client.index(format!("{}{suffix}", shops::INDEX.uid_prefix)),
            client,
            hybrid,
            embedder,
        })
    }

    fn indexes(&self) -> [(&Index, &IndexDefinition); 2] {
        [
            (&self.products, &products::INDEX),
            (&self.shops, &shops::INDEX),
        ]
    }
}

#[derive(Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
    #[serde(default)]
    suggest: bool,
}

pub async fn query(State(app): State<App>, Query(options): Query<SearchQuery>) -> Result {
    let search = &app.search;
    let q = options.q.as_deref().unwrap_or("").trim();
    if q.is_empty() {
        return Ok(Json(json!({"items":[], "shops":[]})));
    }
    let q = text(q, 150)?;

    let shops = search
        .shops
        .search()
        .with_query(q)
        .with_limit(4)
        .with_attributes_to_retrieve(Selectors::Some(shops::INDEX.displayed))
        .build();

    let (items, shops) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::try_join!(
            search.product_results(q, options.suggest),
            shops.execute::<Value>()
        )
    })
    .await
    .map_err(unavailable)?
    .map_err(unavailable)?;

    let shops = shops.hits.into_iter().map(|hit| hit.result);
    let products = hit_ids(items);
    let shops = hit_ids(shops);
    visible_hits(&app.db, &products, &shops).await
}

fn hit_ids(hits: impl IntoIterator<Item = Value>) -> Vec<i64> {
    hits.into_iter()
        .filter_map(|hit| hit["id"].as_i64())
        .collect()
}

pub(super) async fn visible_hits(db: &MySqlPool, products: &[i64], shops: &[i64]) -> Result {
    let mut product_rows = HashMap::new();
    let mut shop_rows = HashMap::new();
    if !products.is_empty() || !shops.is_empty() {
        // Search indexing is asynchronous. Use the source of truth for visibility and prices.
        let rows: Vec<(bool, i64, String, Option<i64>)> = sqlx::query_as(
            "SELECT TRUE,p.id,p.name,(SELECT MIN(v.price) FROM variants v WHERE v.product_id=p.id AND v.enabled)
             FROM JSON_TABLE(?, '$[*]' COLUMNS(id BIGINT PATH '$')) hits
             JOIN products p ON p.id=hits.id JOIN shops s ON s.id=p.shop_id
             WHERE p.status='active' AND NOT s.suspended
             UNION ALL
             SELECT FALSE,s.id,s.name,NULL
             FROM JSON_TABLE(?, '$[*]' COLUMNS(id BIGINT PATH '$')) hits
             JOIN shops s ON s.id=hits.id WHERE NOT s.suspended",
        )
        .bind(json!(products).to_string())
        .bind(json!(shops).to_string())
        .fetch_all(db)
        .await?;
        for (product, id, name, price) in rows {
            if product {
                product_rows.insert(id, json!({"id":id,"name":name,"lowest_price":price}));
            } else {
                shop_rows.insert(id, json!({"id":id,"name":name}));
            }
        }
    }
    let items: Vec<_> = products
        .iter()
        .filter_map(|id| product_rows.remove(id))
        .collect();
    let shops: Vec<_> = shops.iter().filter_map(|id| shop_rows.remove(id)).collect();
    Ok(Json(json!({"items":items,"shops":shops})))
}

impl Search {
    /// Returns storefront name matches with one extra hit to detect a full page.
    pub(crate) async fn admin_product_ids(&self, query: &str, limit: usize) -> Option<Vec<i64>> {
        const MIN_BROAD_MATCH_SCORE: f64 = 0.5;

        #[derive(Deserialize)]
        struct Hit {
            id: i64,
        }

        let mut search = self.products.search();
        search
            .with_query(query)
            .with_limit(limit + 1)
            .with_matching_strategy(MatchingStrategies::ALL)
            .with_attributes_to_search_on(&["name"])
            .with_attributes_to_retrieve(Selectors::Some(&["id"]))
            .with_show_ranking_score(true);

        let results = tokio::time::timeout(Duration::from_millis(500), search.execute::<Hit>())
            .await
            .ok()?
            .ok()?;
        let broad = results.hits.len() > limit;
        // Keep typo tolerance for small result sets. Discard weak matches when they fill the page.
        Some(
            results
                .hits
                .into_iter()
                .filter(|hit| {
                    !broad
                        || hit
                            .ranking_score
                            .is_none_or(|score| score >= MIN_BROAD_MATCH_SCORE)
                })
                .map(|hit| hit.result.id)
                .collect(),
        )
    }

    async fn product_results(
        &self,
        query: &str,
        suggest: bool,
    ) -> std::result::Result<Vec<Value>, MeiliError> {
        let mut request = self.products.search();
        request
            .with_query(query)
            .with_limit(6)
            .with_attributes_to_retrieve(Selectors::Some(products::INDEX.displayed));
        if self.hybrid
            && !suggest
            && let Some(vector) = self.embedder.query(query).await
        {
            let mut hybrid = request.clone();
            hybrid
                .with_hybrid(embeddings::MODEL, 0.35)
                .with_vector(&vector);
            match tokio::time::timeout(Duration::from_millis(750), hybrid.execute::<Value>()).await
            {
                Ok(Ok(result)) => {
                    return Ok(result.hits.into_iter().map(|hit| hit.result).collect());
                }
                Ok(Err(error)) => tracing::debug!(%error, "hybrid search fell back to lexical"),
                Err(_) => tracing::debug!("hybrid search timed out; falling back to lexical"),
            }
        }
        Ok(request
            .execute::<Value>()
            .await?
            .hits
            .into_iter()
            .map(|hit| hit.result)
            .collect())
    }
}

fn unavailable(error: impl std::fmt::Display) -> Error {
    tracing::debug!(%error, "marketplace search unavailable");
    Error(
        StatusCode::SERVICE_UNAVAILABLE,
        "Search is temporarily unavailable".into(),
    )
}

struct IndexDefinition {
    uid_prefix: &'static str,
    entity_type: &'static str,
    settings: fn() -> Settings,
    displayed: &'static [&'static str],
    enqueue: &'static str,
    select_jobs: &'static str,
}

#[derive(sqlx::FromRow)]
struct Job {
    entity_id: i64,
    revision: u64,
    document: Option<String>,
    embedding: Option<sqlx::types::Json<Vec<f32>>>,
}

async fn locked_connection(app: &App) -> anyhow::Result<Option<MySqlConnection>> {
    // Keep the advisory lock on a dedicated session. Connection loss releases it.
    let mut conn = MySqlConnection::connect_with(&app.db.connect_options()).await?;
    Ok(try_lock(&mut conn).await?.then_some(conn))
}

async fn try_lock(conn: &mut MySqlConnection) -> anyhow::Result<bool> {
    let locked: Option<i32> =
        sqlx::query_scalar("SELECT GET_LOCK(CONCAT('tosca-search-', MD5(DATABASE())), 0)")
            .fetch_one(conn)
            .await?;
    Ok(locked == Some(1))
}

async fn wait_for_task(client: &Client, info: TaskInfo) -> anyhow::Result<()> {
    wait_for_task_with_timeout(client, info, TASK_TIMEOUT).await
}

async fn wait_for_task_with_timeout(
    client: &Client,
    info: TaskInfo,
    timeout: Duration,
) -> anyhow::Result<()> {
    let task = info
        .wait_for_completion(client, Some(Duration::from_millis(100)), Some(timeout))
        .await?;
    match task {
        Task::Succeeded { .. } => Ok(()),
        other => anyhow::bail!("Meilisearch did not apply the index update: {other:?}"),
    }
}

async fn ensure_indexes(
    search: &Search,
    conn: &mut MySqlConnection,
    verify_settings: bool,
) -> anyhow::Result<()> {
    for (index, definition) in search.indexes() {
        let created = match search.client.get_index(&index.uid).await {
            Ok(_) => false,
            Err(MeiliError::Meilisearch(error)) if error.error_code == ErrorCode::IndexNotFound => {
                // Persist the full backfill before creating the empty index.
                tracing::info!(index = %index.uid, "creating search index and queuing catalog backfill");
                sqlx::query(definition.enqueue).execute(&mut *conn).await?;
                wait_for_task(
                    &search.client,
                    search.client.create_index(&index.uid, Some("id")).await?,
                )
                .await?;
                true
            }
            Err(error) => return Err(error.into()),
        };
        if created || verify_settings {
            let settings = (definition.settings)();
            if created || !settings_match(index, &settings).await? {
                if !created && definition.entity_type == "product" {
                    // The previous search worker could have drained the migration's backfill
                    // during a rolling upgrade. Requeue with the new index settings.
                    sqlx::query(definition.enqueue).execute(&mut *conn).await?;
                }
                wait_for_task(&search.client, index.set_settings(&settings).await?).await?;
            }
        }
    }
    Ok(())
}

async fn settings_match(index: &Index, settings: &Settings) -> anyhow::Result<bool> {
    let current = match index.get_settings().await {
        Ok(current) => serde_json::to_value(current)?,
        Err(MeiliError::Meilisearch(error)) if error.error_code == ErrorCode::IndexNotFound => {
            return Ok(false);
        }
        Err(error) => return Err(error.into()),
    };
    let expected = serde_json::to_value(settings)?;
    Ok(contains_settings(&current, &expected))
}

fn contains_settings(current: &Value, expected: &Value) -> bool {
    match expected {
        Value::Object(fields) => fields.iter().all(|(key, value)| {
            current
                .get(key)
                .is_some_and(|current| contains_settings(current, value))
        }),
        _ => current == expected,
    }
}

async fn publish_batch(
    search: &Search,
    conn: &mut MySqlConnection,
    index: &Index,
    definition: &IndexDefinition,
    batch_size: u32,
    task_timeout: Duration,
) -> anyhow::Result<usize> {
    let query = sqlx::query_as(definition.select_jobs);
    let query = if definition.entity_type == "product" {
        query.bind(embeddings::MODEL)
    } else {
        query
    };
    let jobs: Vec<Job> = query.bind(batch_size).fetch_all(&mut *conn).await?;
    if jobs.is_empty() {
        return Ok(0);
    }
    let count = jobs.len();

    let mut upserts = Vec::new();
    let mut removed = Vec::new();
    let mut products = Vec::new();
    for job in &jobs {
        match &job.document {
            Some(document) => {
                let mut document = serde_json::from_str::<Value>(document)?;
                if definition.entity_type == "product" {
                    products.push(job.entity_id);
                    document["_vectors"] =
                        json!({(embeddings::MODEL): job.embedding.as_ref().map(|v| &v.0)});
                }
                upserts.push(document);
            }
            None => removed.push(job.entity_id),
        }
    }
    for ids in products.chunks(SQL_BATCH_SIZE) {
        embeddings::enqueue(&mut *conn, ids).await?;
    }
    if !removed.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index.delete_documents(&removed).await?,
            task_timeout,
        )
        .await?;
    }
    if !upserts.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index.add_or_replace(&upserts, Some("id")).await?,
            task_timeout,
        )
        .await?;
    }

    // A newer revision can arrive while Meilisearch is indexing. Only acknowledge the published revisions.
    for batch in jobs.chunks(SQL_BATCH_SIZE) {
        let mut delete =
            QueryBuilder::<MySql>::new("DELETE FROM search_sync_jobs WHERE entity_type=");
        delete
            .push_bind(definition.entity_type)
            .push(" AND (entity_id,revision) IN (");
        let mut pairs = delete.separated(",");
        for job in batch {
            pairs
                .push("(")
                .push_bind_unseparated(job.entity_id)
                .push_unseparated(",")
                .push_bind_unseparated(job.revision)
                .push_unseparated(")");
        }
        pairs.push_unseparated(")");
        delete.build().persistent(false).execute(&mut *conn).await?;
    }
    Ok(count)
}

async fn pending(conn: &mut MySqlConnection) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM search_sync_jobs)")
            .fetch_one(conn)
            .await?,
    )
}

async fn publish_pending(search: &Search, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    for (index, definition) in search.indexes() {
        tokio::time::timeout(
            Duration::from_secs(40),
            publish_batch(
                search,
                conn,
                index,
                definition,
                LIVE_BATCH_SIZE,
                TASK_TIMEOUT,
            ),
        )
        .await??;
    }
    Ok(())
}

async fn prepare_locked(search: &Search, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(90), ensure_indexes(search, conn, true))
        .await
        .context("Timed out preparing Meilisearch index settings")??;
    while pending(conn).await? {
        publish_pending(search, conn)
            .await
            .context("Backfilling Meilisearch from MariaDB")?;
    }
    Ok(())
}

async fn wait_for_startup_access(
    search: &Search,
    conn: &mut MySqlConnection,
) -> anyhow::Result<bool> {
    loop {
        if try_lock(conn).await? {
            return Ok(true);
        }
        // A worker serving the same database may already have prepared these indexes.
        if !pending(conn).await?
            && settings_match(&search.shops, &(shops::INDEX.settings)()).await?
            && (settings_match(&search.products, &(products::INDEX.settings)()).await?
                || settings_match(&search.products, &products::legacy_settings()).await?)
        {
            return Ok(false);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

pub async fn prepare(app: &App) -> anyhow::Result<()> {
    let mut conn = MySqlConnection::connect_with(&app.db.connect_options()).await?;
    let acquired = tokio::time::timeout(
        STARTUP_LOCK_TIMEOUT,
        wait_for_startup_access(&app.search, &mut conn),
    )
    .await
    .context("Search initialization timed out. Check Meilisearch and stop any backend using a different MEILI_INDEX_SUFFIX for this database")??;
    if acquired {
        prepare_locked(&app.search, &mut conn).await.context(
            "Preparing search indexes. Check MEILI_URL, MEILI_KEY, and MEILI_INDEX_SUFFIX",
        )?;
    }
    conn.close().await?;
    tracing::info!(products = %app.search.products.uid, shops = %app.search.shops.uid, "search indexes ready");
    Ok(())
}

async fn session(app: &App) -> anyhow::Result<()> {
    let Some(mut conn) = locked_connection(app).await? else {
        return Ok(());
    };
    ensure_indexes(&app.search, &mut conn, true).await?;
    let mut last_index_check = tokio::time::Instant::now();
    loop {
        let queued = pending(&mut conn).await?;
        let verify_settings = last_index_check.elapsed() >= INDEX_CHECK_INTERVAL;
        if queued || verify_settings {
            // Check existence before publishing, so a removed index is fully backfilled.
            tokio::time::timeout(
                Duration::from_secs(25),
                ensure_indexes(&app.search, &mut conn, verify_settings),
            )
            .await??;
            if verify_settings {
                last_index_check = tokio::time::Instant::now();
            }
            publish_pending(&app.search, &mut conn).await?;
        }
        if !queued {
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

pub async fn index_worker(app: App) {
    loop {
        if let Err(error) = session(&app).await {
            tracing::warn!(%error, "search sync paused; queued changes will be retried");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn rebuild(app: &App) -> anyhow::Result<()> {
    for (_, definition) in app.search.indexes() {
        sqlx::query(definition.enqueue).execute(&app.db).await?;
    }
    // A running API may already own the lock and drain the queued rebuild.
    if let Some(mut conn) = locked_connection(app).await? {
        prepare_locked(&app.search, &mut conn).await?;
    }
    Ok(())
}

pub(super) async fn sync_bulk(db: &sqlx::MySqlPool, search: &Search) -> anyhow::Result<()> {
    let mut conn = MySqlConnection::connect_with(&db.connect_options()).await?;
    loop {
        if try_lock(&mut conn).await? {
            break;
        }
        // The API owns the same queue while it is running.
        if !pending(&mut conn).await?
            && settings_match(&search.shops, &(shops::INDEX.settings)()).await?
            && settings_match(&search.products, &(products::INDEX.settings)()).await?
        {
            conn.close().await?;
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    ensure_indexes(search, &mut conn, true).await?;
    let mut processed = 0;
    while pending(&mut conn).await? {
        for (index, definition) in search.indexes() {
            let count = publish_batch(
                search,
                &mut conn,
                index,
                definition,
                BULK_BATCH_SIZE,
                BULK_TASK_TIMEOUT,
            )
            .await
            .with_context(|| format!("Syncing index {}", index.uid))?;
            if count > 0 && definition.entity_type == "product" {
                processed += count;
                if processed.is_multiple_of(50_000) {
                    tracing::info!(processed, index = %index.uid, "Amazon import search updates applied");
                }
            }
        }
    }
    conn.close().await?;
    Ok(())
}
