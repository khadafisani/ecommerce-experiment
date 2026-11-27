use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;
use axum::extract::{Query, State};
use futures_util::io::Cursor;
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

mod canonical;
mod extractor;
mod keywords;
mod products;
mod shops;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const INDEX_CHECK_INTERVAL: Duration = Duration::from_secs(15);
const STARTUP_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const TASK_TIMEOUT: Duration = Duration::from_secs(15);
const DOCUMENT_TASK_TIMEOUT: Duration = Duration::from_secs(600);
const BATCH_SIZE: u32 = 20_000;
// Leave headroom below Meilisearch's default 100 MB HTTP payload limit.
const MAX_DOCUMENT_PAYLOAD_BYTES: usize = 90 * 1024 * 1024;
const SQL_BATCH_SIZE: usize = 10_000;

#[derive(Clone)]
pub struct Search {
    client: Client,
    products: Index,
    shops: Index,
    keywords: Index,
    canonical: Index,
    hybrid: bool,
    embedder: embeddings::Embedder,
    query_logging: Arc<tokio::sync::Semaphore>,
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
            keywords: client.index(format!("{}{suffix}", keywords::INDEX)),
            canonical: client.index(format!("{}{suffix}", canonical::INDEX)),
            client,
            hybrid,
            embedder,
            query_logging: Arc::new(tokio::sync::Semaphore::new(32)),
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
        return Ok(Json(json!({"items":[], "shops":[], "keywords":[]})));
    }
    let q = text(q, 150)?;

    let shops = search
        .shops
        .search()
        .with_query(q)
        .with_limit(4)
        .with_attributes_to_retrieve(Selectors::Some(shops::INDEX.displayed))
        .build();

    if !options.suggest
        && let Ok(permit) = Arc::clone(&search.query_logging).try_acquire_owned()
    {
        let db = app.db.clone();
        let submitted = q.to_owned();
        tokio::spawn(async move {
            let _permit = permit;
            match tokio::time::timeout(
                Duration::from_millis(500),
                keywords::record(&db, &submitted),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::debug!(%error, "could not record submitted search"),
                Err(_) => tracing::debug!("recording submitted search timed out"),
            }
        });
    }
    let shops = async {
        let shops = shops.execute::<Value>().await.map_err(unavailable)?;
        Ok::<_, Error>(hit_ids(shops.hits.into_iter().map(|hit| hit.result)))
    };
    let products = async {
        if options.suggest {
            let suggestions = keywords::suggest(search, q).await;
            let fallback = if suggestions.is_empty() {
                search.listing_suggestions(q).await.map_err(unavailable)?
            } else {
                Vec::new()
            };
            return Ok((Vec::new(), fallback, suggestions, None));
        }
        let mut correction = keywords::correction(search, q).await;
        let term = correction.as_deref().unwrap_or(q);
        let mut canonical = Vec::new();
        match canonical::hits(search, &app.db, term, correction.is_none()).await {
            Ok((hits, hint)) => {
                canonical = canonical::offers(&app.db, hits, hint.as_deref().unwrap_or(term))
                    .await
                    .map_err(unavailable)?;
                if !canonical.is_empty() && hint.is_some() {
                    correction = hint;
                }
            }
            Err(error) => {
                tracing::debug!(%error, "canonical search unavailable; using listing keywords")
            }
        }
        let fallback = if canonical.is_empty() {
            let term = correction.as_deref().unwrap_or(q);
            let filters = canonical::offer_filters(term);
            if filters.constrained() {
                let ids = hit_ids(
                    search
                        .listing_results_with_limit(&filters.terms, 24, MatchingStrategies::ALL)
                        .await
                        .map_err(unavailable)?,
                );
                canonical = canonical::listing_offers(&app.db, &ids, term)
                    .await
                    .map_err(unavailable)?;
                Vec::new()
            } else {
                hit_ids(search.listing_results(term).await.map_err(unavailable)?)
            }
        } else {
            Vec::new()
        };
        Ok((canonical, fallback, Vec::new(), correction))
    };
    let ((canonical, fallback, suggestions, corrected), shops) =
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::try_join!(products, shops)
        })
        .await
        .map_err(unavailable)??;
    let Json(mut response) = visible_hits(&app.db, &fallback, &shops).await?;
    if !canonical.is_empty() {
        response["items"] = json!(canonical);
    }
    response["keywords"] = json!(suggestions);
    if let Some(corrected) = corrected {
        response["corrected_query"] = json!(corrected);
    }
    Ok(Json(response))
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

    async fn listing_results(&self, query: &str) -> std::result::Result<Vec<Value>, MeiliError> {
        self.listing_results_with_limit(query, 6, MatchingStrategies::LAST)
            .await
    }

    async fn listing_suggestions(&self, query: &str) -> std::result::Result<Vec<i64>, MeiliError> {
        const FIELDS: &[&str] = &["name", "brand", "category"];
        let mut request = self.products.search();
        request
            .with_query(query)
            .with_limit(24)
            .with_matching_strategy(MatchingStrategies::ALL)
            .with_attributes_to_retrieve(Selectors::Some(&["id", "name", "brand", "category"]))
            .with_show_matches_position(true);
        let result = request.execute::<Value>().await?;
        Ok(hit_ids(
            result
                .hits
                .into_iter()
                .filter(|hit| keywords::leading_match(hit.matches_position.as_ref(), FIELDS))
                .take(6)
                .map(|hit| hit.result),
        ))
    }

    async fn listing_results_with_limit(
        &self,
        query: &str,
        limit: usize,
        matching: MatchingStrategies,
    ) -> std::result::Result<Vec<Value>, MeiliError> {
        let mut request = self.products.search();
        request
            .with_query(query)
            .with_limit(limit)
            .with_matching_strategy(matching)
            .with_attributes_to_retrieve(Selectors::Some(products::INDEX.displayed));
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
) -> anyhow::Result<usize> {
    let jobs: Vec<Job> = sqlx::query_as(definition.select_jobs)
        .bind(BATCH_SIZE)
        .fetch_all(&mut *conn)
        .await?;
    if jobs.is_empty() {
        return Ok(0);
    }
    let mut published = jobs.len();
    let mut ndjson = Vec::new();
    let mut removed = Vec::new();
    let mut products = Vec::new();
    for (position, job) in jobs.iter().enumerate() {
        match &job.document {
            Some(document) => {
                let mut document = serde_json::from_str::<Value>(document)?;
                if definition.entity_type == "product" {
                    // The listing index handles lexical fallback. Remove old vectors on update.
                    document["_vectors"] = json!({(embeddings::MODEL):null});
                }
                let line_start = ndjson.len();
                serde_json::to_writer(&mut ndjson, &document)?;
                ndjson.push(b'\n');
                if ndjson.len() > MAX_DOCUMENT_PAYLOAD_BYTES {
                    anyhow::ensure!(
                        line_start > 0,
                        "Search document {} exceeds the payload limit",
                        job.entity_id
                    );
                    ndjson.truncate(line_start);
                    published = position;
                    break;
                }
                if definition.entity_type == "product" {
                    products.push(job.entity_id);
                }
            }
            None => removed.push(job.entity_id),
        }
    }
    if definition.entity_type == "product" {
        for ids in products.chunks(SQL_BATCH_SIZE) {
            embeddings::enqueue(&mut *conn, ids).await?;
        }
        let ids: Vec<_> = jobs[..published].iter().map(|job| job.entity_id).collect();
        for ids in ids.chunks(SQL_BATCH_SIZE) {
            sqlx::query(
                "INSERT INTO search_catalog_jobs (product_id)
                 SELECT hits.id FROM JSON_TABLE(?, '$[*]' COLUMNS(id BIGINT PATH '$')) hits
                 JOIN search_catalog_cursor c ON c.id=1 AND (c.backfill_complete OR hits.id<=c.last_product_id)
                 ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6)",
            )
            .bind(json!(ids).to_string())
            .execute(&mut *conn)
            .await?;
        }
    }
    if !removed.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index.delete_documents(&removed).await?,
            DOCUMENT_TASK_TIMEOUT,
        )
        .await?;
    }
    if !ndjson.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index
                .add_or_update_unchecked_payload(
                    Cursor::new(ndjson),
                    "application/x-ndjson",
                    Some("id"),
                )
                .await?,
            DOCUMENT_TASK_TIMEOUT,
        )
        .await?;
    }

    // A newer revision can arrive while Meilisearch is indexing. Only acknowledge the published revisions.
    for batch in jobs[..published].chunks(SQL_BATCH_SIZE) {
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
    Ok(published)
}

async fn pending(conn: &mut MySqlConnection) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM search_sync_jobs)")
            .fetch_one(conn)
            .await?,
    )
}

async fn publish_pending(search: &Search, conn: &mut MySqlConnection) -> anyhow::Result<usize> {
    let mut products = 0;
    for (index, definition) in search.indexes() {
        let count = tokio::time::timeout(
            DOCUMENT_TASK_TIMEOUT * 2 + Duration::from_secs(30),
            publish_batch(search, conn, index, definition),
        )
        .await
        .with_context(|| format!("Timed out syncing index {}", index.uid))?
        .with_context(|| format!("Syncing index {}", index.uid))?;
        if definition.entity_type == "product" {
            products = count;
        }
    }
    Ok(products)
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

pub async fn cleanup_worker(app: App) {
    canonical::worker(app.db, app.search).await;
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
        let previous = processed;
        processed += publish_pending(search, &mut conn).await?;
        if processed / 50_000 > previous / 50_000 {
            tracing::info!(processed, index = %search.products.uid, "Amazon import search updates applied");
        }
    }
    conn.close().await?;
    Ok(())
}
