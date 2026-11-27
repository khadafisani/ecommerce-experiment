use std::{collections::HashMap, time::Duration};

use meilisearch_sdk::{
    errors::Error as MeiliError,
    indexes::Index,
    search::{MatchingStrategies, Selectors},
    settings::{Embedder, EmbedderSource, Settings},
};
use serde::Deserialize;
use sqlx::{Connection, MySql, MySqlConnection, MySqlPool, QueryBuilder};

use super::{
    DOCUMENT_TASK_TIMEOUT, Search,
    extractor::{Entities, Extractor, Task},
    keywords, wait_for_task_with_timeout,
};
use crate::{Value, embeddings, json};

pub(super) const INDEX: &str = "tosca_market_canonical_products_v1";
const BATCH_SIZE: usize = 32;
const CATALOG_EXTRACTION_VERSION: &str = "gliner25-v1";

pub(super) fn settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(["name", "brand", "options", "category"])
        .with_embedders(HashMap::from([(
            embeddings::MODEL,
            Embedder {
                source: EmbedderSource::UserProvided,
                dimensions: Some(embeddings::DIMENSIONS),
                ..Default::default()
            },
        )]))
}

pub(super) async fn worker(db: MySqlPool, search: Search) {
    let mut extractor = match Extractor::new() {
        Ok(extractor) => extractor,
        Err(error) => {
            tracing::error!(%error, "could not configure search cleanup");
            return;
        }
    };
    loop {
        if let Err(error) = session(&db, &search, &mut extractor).await {
            tracing::warn!(%error, "canonical search worker paused; queued changes will retry");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn session(db: &MySqlPool, search: &Search, extractor: &mut Extractor) -> anyhow::Result<()> {
    let mut conn = MySqlConnection::connect_with(&db.connect_options()).await?;
    let locked: Option<i32> =
        sqlx::query_scalar("SELECT GET_LOCK(CONCAT('tosca-keywords-', MD5(DATABASE())),0)")
            .fetch_one(&mut conn)
            .await?;
    if locked != Some(1) {
        return Ok(());
    }
    let mut catalog_cache = HashMap::new();
    let mut last_check = None;
    let mut last_cleanup = None;
    let mut catalog_retry_at = tokio::time::Instant::now();
    loop {
        let now = tokio::time::Instant::now();
        if last_check.is_none_or(|last| now - last >= Duration::from_secs(30)) {
            if keywords::ensure_index(search, &search.keywords, keywords::settings()).await? {
                sqlx::query("UPDATE search_keyword_terms SET dirty=TRUE")
                    .execute(&mut conn)
                    .await?;
            }
            if keywords::ensure_index(search, &search.canonical, settings()).await? {
                sqlx::query("UPDATE search_canonical_products SET dirty=TRUE")
                    .execute(&mut conn)
                    .await?;
            }
            last_check = Some(now);
        }
        if last_cleanup.is_none_or(|last| now - last >= Duration::from_secs(30)) {
            if let Err(error) = keywords::clean_queries(&mut conn, extractor, search).await {
                tracing::warn!(error = %format_args!("{error:#}"), "search query cleanup will retry");
            }
            last_cleanup = Some(now);
        }

        let catalog_changed = if now >= catalog_retry_at {
            match process_catalog(&mut conn, extractor, &mut catalog_cache).await {
                Ok(changed) => changed,
                Err(error) => {
                    tracing::warn!(error = %format_args!("{error:#}"), "catalog cleanup will retry");
                    catalog_retry_at = tokio::time::Instant::now() + Duration::from_secs(30);
                    false
                }
            }
        } else {
            false
        };
        let embedded = search.hybrid && embed_pending(&mut conn, search).await?;
        let keywords_changed = keywords::publish(&mut conn, search).await?;
        let products_changed = publish(&mut conn, search).await?;
        if !(catalog_changed || embedded || keywords_changed || products_changed) {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

async fn queue_backfill(conn: &mut MySqlConnection) -> anyhow::Result<bool> {
    let mut tx = conn.begin().await?;
    let (cursor, complete): (i64, bool) = sqlx::query_as(
        "SELECT last_product_id,backfill_complete FROM search_catalog_cursor WHERE id=1 FOR UPDATE",
    )
    .fetch_one(&mut *tx)
    .await?;
    if complete {
        tx.commit().await?;
        return Ok(false);
    }
    let next: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(id) FROM (SELECT id FROM products WHERE id>? ORDER BY id LIMIT 128) batch",
    )
    .bind(cursor)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(next) = next {
        sqlx::query(
            "INSERT IGNORE INTO search_catalog_jobs(product_id)
             SELECT id FROM products WHERE id>? AND id<=?",
        )
        .bind(cursor)
        .bind(next)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE search_catalog_cursor SET last_product_id=?,backfill_complete=? WHERE id=1",
    )
    .bind(next.unwrap_or(cursor))
    .bind(next.is_none())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(next.is_some())
}

#[derive(sqlx::FromRow)]
struct Listing {
    name: String,
    brand: String,
    category: String,
}

#[derive(sqlx::FromRow)]
struct Variant {
    id: i64,
    name: String,
    subvariant: String,
}

struct CatalogJob {
    id: i64,
    revision: u64,
    listing: Option<Listing>,
    variants: Vec<Variant>,
    fingerprint: String,
}

struct CatalogText {
    name: String,
    suggestion: Option<String>,
    brand: Option<String>,
}

async fn process_catalog(
    conn: &mut MySqlConnection,
    extractor: &mut Extractor,
    cache: &mut HashMap<String, CatalogText>,
) -> anyhow::Result<bool> {
    let queued: Vec<(i64, u64)> = sqlx::query_as(
        "SELECT product_id,revision FROM search_catalog_jobs ORDER BY updated_at,product_id LIMIT ?",
    )
    .bind(BATCH_SIZE as u32)
    .fetch_all(&mut *conn)
    .await?;
    if queued.is_empty() {
        return queue_backfill(conn).await;
    }
    let mut jobs = Vec::new();
    let mut titles = Vec::new();
    let mut options = Vec::new();
    for (id, revision) in queued {
        let listing: Option<Listing> = sqlx::query_as(
            "SELECT p.name,COALESCE(b.name,'') brand,COALESCE(c.name,'') category FROM products p
             JOIN shops s ON s.id=p.shop_id
             LEFT JOIN taxonomy b ON b.id=p.brand_id LEFT JOIN taxonomy c ON c.id=p.category_id
             WHERE p.id=? AND p.status='active' AND NOT s.suspended",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        let variants: Vec<Variant> = if listing.is_some() {
            sqlx::query_as(
                "SELECT id,name,subvariant FROM variants WHERE product_id=? AND enabled ORDER BY id",
            )
            .bind(id)
            .fetch_all(&mut *conn)
            .await?
        } else {
            Vec::new()
        };
        let fingerprint = listing.as_ref().map_or_else(String::new, |listing| {
            let variants: Vec<_> = variants
                .iter()
                .map(|v| (v.id, &v.name, &v.subvariant))
                .collect();
            keywords::id(
                &json!([
                    CATALOG_EXTRACTION_VERSION,
                    listing.name,
                    listing.brand,
                    listing.category,
                    variants
                ])
                .to_string(),
            )
        });
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT catalog_fingerprint FROM search_catalog_sources WHERE product_id=? AND variant_id=0",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        if existing.as_deref().unwrap_or("") == fingerprint {
            acknowledge(conn, id, revision).await?;
            continue;
        }
        if let Some(listing) = &listing {
            titles.push(listing.name.clone());
            options.extend(variants.iter().filter_map(variant_input));
        }
        jobs.push(CatalogJob {
            id,
            revision,
            listing,
            variants,
            fingerprint,
        });
    }
    if cache.len() >= 4096 {
        cache.clear();
    }
    clean_catalog_text(extractor, cache, titles, false).await?;
    clean_catalog_text(extractor, cache, options, true).await?;
    for job in jobs {
        apply_catalog(conn, job, cache).await?;
    }
    Ok(true)
}

fn variant_input(variant: &Variant) -> Option<String> {
    let input = format!("{} {}", variant.name, variant.subvariant);
    let text = keywords::normalize(&input);
    (!matches!(text.as_str(), "" | "standard" | "default" | "standar")).then_some(input)
}

async fn clean_catalog_text(
    extractor: &mut Extractor,
    cache: &mut HashMap<String, CatalogText>,
    inputs: Vec<String>,
    variant: bool,
) -> anyhow::Result<()> {
    let prefix = if variant { "variant:" } else { "product:" };
    let mut inputs: Vec<_> = inputs
        .into_iter()
        .map(|text| (keywords::normalize(&text), text))
        .collect();
    inputs.sort_unstable();
    inputs.dedup_by(|a, b| a.0 == b.0);
    inputs.retain(|(text, _)| !text.is_empty() && !cache.contains_key(&format!("{prefix}{text}")));
    inputs.retain(|(text, _)| {
        if needs_extraction(text, variant) {
            true
        } else {
            cache.insert(
                format!("{prefix}{text}"),
                catalog_text(text, Entities::default(), variant),
            );
            false
        }
    });
    let task = if variant {
        Task::Variant
    } else {
        Task::Product
    };
    for inputs in inputs.chunks(BATCH_SIZE) {
        let raw: Vec<_> = inputs.iter().map(|(_, text)| text.clone()).collect();
        let entities = extractor.extract(task, &raw).await?;
        for ((original, _), entities) in inputs.iter().zip(entities) {
            cache.insert(
                format!("{prefix}{original}"),
                catalog_text(original, entities, variant),
            );
        }
    }
    Ok(())
}

fn needs_extraction(text: &str, variant: bool) -> bool {
    !keywords::compact_phrase(text)
        || if variant {
            text.split_whitespace().next().is_some_and(option_label)
        } else {
            text.split_whitespace().any(|word| {
                is_storage(word)
                    || condition(word).is_some()
                    || matches!(
                        word,
                        "with"
                            | "for"
                            | "compatible"
                            | "dengan"
                            | "untuk"
                            | "case"
                            | "casing"
                            | "promo"
                            | "sale"
                    )
            })
        }
}

fn option_label(word: &str) -> bool {
    matches!(
        word,
        "warna"
            | "color"
            | "kapasitas"
            | "storage"
            | "ukuran"
            | "size"
            | "varian"
            | "variant"
            | "kondisi"
            | "condition"
    )
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    !phrase.is_empty() && format!(" {text} ").contains(&format!(" {phrase} "))
}

fn catalog_text(original: &str, entities: Entities, variant: bool) -> CatalogText {
    let candidate = if variant {
        entities.option
    } else {
        entities.product
    };
    let candidate = candidate.map(|text| keywords::normalize(&text));
    let name = candidate
        .as_ref()
        .filter(|name| safe_title(original, name, variant))
        .map_or_else(|| original.to_owned(), Clone::clone);
    if variant {
        return CatalogText {
            name,
            suggestion: None,
            brand: None,
        };
    }

    // Text after these markers describes features or compatible items.
    let head_end = [
        " with ",
        " for ",
        " compatible ",
        " fits ",
        " dengan ",
        " untuk ",
    ]
    .iter()
    .filter_map(|marker| original.find(marker))
    .min()
    .unwrap_or(original.len());
    let head = &original[..head_end];
    let accessory = head.split_whitespace().next().filter(|word| {
        matches!(
            *word,
            "case" | "casing" | "cover" | "charger" | "pelindung" | "sarung"
        )
    });
    let compatibility = entities
        .compatibility
        .map(|text| keywords::normalize(&text));
    let valid_span = |phrase: &String| {
        keywords::compact_phrase(phrase)
            && contains_phrase(head, phrase)
            && !compatibility
                .as_ref()
                .is_some_and(|target| contains_phrase(target, phrase))
    };
    let suggestion = candidate
        .filter(|phrase| {
            valid_span(phrase) && accessory.is_none_or(|kind| contains_phrase(phrase, kind))
        })
        .or_else(|| {
            entities
                .product_type
                .map(|text| keywords::normalize(&text))
                .filter(valid_span)
        })
        .or_else(|| accessory.map(str::to_owned));
    let brand = entities
        .brand
        .map(|text| keywords::normalize(&text))
        .filter(|phrase| accessory.is_none() && valid_span(phrase));
    CatalogText {
        name,
        suggestion,
        brand,
    }
}

fn is_storage(word: &str) -> bool {
    word.strip_suffix("gb")
        .or_else(|| word.strip_suffix("tb"))
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit())
        })
}

fn condition(word: &str) -> Option<&'static str> {
    match word {
        "bekas" | "second" | "used" | "preowned" => Some("used"),
        "baru" | "new" => Some("new"),
        _ => None,
    }
}

fn safe_title(original: &str, candidate: &str, variant: bool) -> bool {
    if candidate.is_empty() {
        return false;
    }
    let mut retained = candidate.split_whitespace().peekable();
    let mut words = original.split_whitespace().peekable();
    let first_word = words.peek().copied();
    while let Some(word) = words.next() {
        if retained.peek() == Some(&word) {
            retained.next();
        } else if !variant
            && word.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(words.peek(), Some(&"gb" | &"tb"))
        {
            words.next();
        } else if variant {
            if first_word != Some(word) || !option_label(word) {
                return false;
            }
        } else if !(is_storage(word)
            || (retained.peek().is_none() && condition(word).is_some())
            || matches!(
                word,
                "promo" | "ready" | "stok" | "stock" | "sale" | "termurah"
            ))
        {
            return false;
        }
    }
    retained.peek().is_none()
}

fn iphone_family(name: &str) -> Option<String> {
    let words: Vec<_> = name.split_whitespace().collect();
    let words = words.strip_prefix(&["apple"]).unwrap_or(&words);
    let ["iphone", generation, suffix @ ..] = words else {
        return None;
    };
    if generation.len() != 2 || !generation.bytes().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    matches!(suffix, ["pro", "max"] | ["pro"] | ["plus"] | ["mini"])
        .then(|| format!("iphone {}", suffix.join(" ")))
}

async fn apply_catalog(
    conn: &mut MySqlConnection,
    job: CatalogJob,
    cache: &HashMap<String, CatalogText>,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    if !acknowledge(&mut tx, job.id, job.revision).await? {
        return Ok(());
    }
    sqlx::query(
        "UPDATE search_canonical_products c
         JOIN search_catalog_sources s ON s.canonical_id=c.id AND s.variant_id=0
         SET c.dirty=TRUE WHERE s.product_id=?",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE search_keyword_terms k JOIN search_catalog_sources s ON s.keyword_id=k.id
         SET k.dirty=TRUE WHERE s.product_id=?",
    )
    .bind(job.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM search_catalog_sources WHERE product_id=?")
        .bind(job.id)
        .execute(&mut *tx)
        .await?;
    if let Some(listing) = job.listing {
        let title = keywords::normalize(&listing.name);
        let extracted = cache.get(&format!("product:{title}"));
        let cleaned = extracted.map_or(title.as_str(), |entry| entry.name.as_str());
        if !cleaned.is_empty() {
            let brand = if listing.brand.is_empty() {
                extracted
                    .and_then(|entry| entry.brand.clone())
                    .unwrap_or_default()
            } else {
                keywords::normalize(&listing.brand)
            };
            let without_brand = cleaned
                .strip_prefix(&brand)
                .and_then(|rest| rest.strip_prefix(' '))
                .filter(|rest| !brand.is_empty() && !rest.is_empty())
                .unwrap_or(cleaned);
            let name: String = without_brand.chars().take(180).collect();
            let group = keywords::id(
                &json!([name, brand, keywords::normalize(&listing.category)]).to_string(),
            );
            let keyword = ensure_keyword(&mut tx, &name).await?;
            sqlx::query(
                "INSERT INTO search_canonical_products (id,keyword_id,brand,category) VALUES (?,?,?,?)
                 ON DUPLICATE KEY UPDATE dirty=TRUE",
            ).bind(&group).bind(&keyword).bind(&brand).bind(&listing.category)
                .execute(&mut *tx).await?;
            sqlx::query("INSERT INTO search_catalog_sources VALUES (?,0,?,?,?)")
                .bind(job.id)
                .bind(&group)
                .bind(&keyword)
                .bind(job.fingerprint)
                .execute(&mut *tx)
                .await?;
            if let Some(family) = iphone_family(&name) {
                let family = ensure_keyword(&mut tx, &family).await?;
                sqlx::query("INSERT INTO search_catalog_sources (product_id,variant_id,canonical_id,keyword_id)
                             VALUES (?,-1,?,?)")
                    .bind(job.id).bind(&group).bind(family).execute(&mut *tx).await?;
            }
            for (source_id, phrase) in [
                (
                    -2,
                    extracted
                        .and_then(|entry| entry.suggestion.as_deref())
                        .unwrap_or_default(),
                ),
                (-3, brand.as_str()),
            ] {
                if phrase != name && keywords::compact_phrase(phrase) {
                    let keyword = ensure_keyword(&mut tx, phrase).await?;
                    sqlx::query("INSERT INTO search_catalog_sources (product_id,variant_id,canonical_id,keyword_id) VALUES (?,?,?,?)")
                        .bind(job.id).bind(source_id).bind(&group).bind(keyword).execute(&mut *tx).await?;
                }
            }
            for variant in job.variants {
                let Some(option) = variant_input(&variant) else {
                    continue;
                };
                let option = keywords::normalize(&option);
                let option = cache
                    .get(&format!("variant:{option}"))
                    .map_or(option.as_str(), |entry| entry.name.as_str());
                let phrase: String = format!("{name} {option}").chars().take(180).collect();
                let keyword = ensure_keyword(&mut tx, phrase.trim_end()).await?;
                sqlx::query("INSERT INTO search_catalog_sources (product_id,variant_id,canonical_id,keyword_id) VALUES (?,?,?,?)")
                    .bind(job.id).bind(variant.id).bind(&group).bind(keyword).execute(&mut *tx).await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn ensure_keyword(conn: &mut MySqlConnection, phrase: &str) -> anyhow::Result<String> {
    let key = keywords::id(phrase);
    sqlx::query("INSERT INTO search_keyword_terms (id,phrase) VALUES (?,?) ON DUPLICATE KEY UPDATE dirty=TRUE")
        .bind(&key).bind(phrase).execute(conn).await?;
    Ok(key)
}

async fn acknowledge(conn: &mut MySqlConnection, id: i64, revision: u64) -> anyhow::Result<bool> {
    Ok(
        sqlx::query("DELETE FROM search_catalog_jobs WHERE product_id=? AND revision=?")
            .bind(id)
            .bind(revision)
            .execute(conn)
            .await?
            .rows_affected()
            == 1,
    )
}

async fn embed_pending(conn: &mut MySqlConnection, search: &Search) -> anyhow::Result<bool> {
    let records: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.id, CONCAT('passage: ',k.phrase,' | ',c.brand,' | ',c.category,' | ',
             COALESCE((SELECT LEFT(p.description,400) FROM search_catalog_sources s
                 JOIN products p ON p.id=s.product_id
                 WHERE s.canonical_id=c.id AND s.variant_id=0 ORDER BY s.product_id LIMIT 1),''))
         FROM search_canonical_products c JOIN search_keyword_terms k ON k.id=c.keyword_id
         WHERE NOT c.embedded AND EXISTS (SELECT 1 FROM search_catalog_sources s WHERE s.canonical_id=c.id AND s.variant_id=0)
         ORDER BY c.id LIMIT 8",
    ).fetch_all(&mut *conn).await?;
    if records.is_empty() {
        return Ok(false);
    }
    let (keys, texts): (Vec<_>, Vec<_>) = records.into_iter().unzip();
    let Some(vectors) = search.embedder.background_batch(texts).await? else {
        return Ok(false);
    };
    for (key, vector) in keys.into_iter().zip(vectors) {
        sqlx::query(
            "UPDATE search_canonical_products SET embedding=?,embedded=TRUE,dirty=TRUE WHERE id=?",
        )
        .bind(sqlx::types::Json(vector))
        .bind(key)
        .execute(&mut *conn)
        .await?;
    }
    Ok(true)
}

#[derive(sqlx::FromRow)]
struct Document {
    id: String,
    phrase: String,
    brand: String,
    category: String,
    active: bool,
    embedding: Option<sqlx::types::Json<Vec<f32>>>,
    options: Option<sqlx::types::Json<Vec<String>>>,
}

async fn publish(conn: &mut MySqlConnection, search: &Search) -> anyhow::Result<bool> {
    let rows: Vec<Document> = sqlx::query_as(
        "SELECT c.id,k.phrase,c.brand,c.category,c.embedding,
           EXISTS(SELECT 1 FROM search_catalog_sources s WHERE s.canonical_id=c.id AND s.variant_id=0) active,
           (SELECT JSON_ARRAYAGG(DISTINCT opt.phrase ORDER BY opt.phrase LIMIT 48)
            FROM search_catalog_sources s JOIN search_keyword_terms opt ON opt.id=s.keyword_id
            WHERE s.canonical_id=c.id AND s.variant_id>0) options
         FROM search_canonical_products c JOIN search_keyword_terms k ON k.id=c.keyword_id
         WHERE c.dirty ORDER BY c.id LIMIT 128",
    ).fetch_all(&mut *conn).await?;
    let mut documents = Vec::new();
    let mut deleted = Vec::new();
    let mut ids = Vec::new();
    for row in &rows {
        if row.active {
            ids.push(&row.id);
            documents.push(json!({
                "id":row.id,"name":row.phrase,"brand":row.brand,"category":row.category,
                "options":row.options.as_ref().map(|value| &value.0),
                "_vectors":{(embeddings::MODEL):row.embedding.as_ref().map(|value| &value.0)}
            }));
        } else {
            deleted.push(&row.id);
        }
    }
    publish_documents(search, &search.canonical, &documents, &deleted).await?;
    if !ids.is_empty() {
        sqlx::query(
            "UPDATE search_canonical_products SET dirty=FALSE
             WHERE id IN (SELECT id FROM JSON_TABLE(?, '$[*]' COLUMNS(id CHAR(64) PATH '$')) ids)",
        )
        .bind(json!(ids).to_string())
        .execute(&mut *conn)
        .await?;
    }
    if !deleted.is_empty() {
        sqlx::query(
            "DELETE FROM search_canonical_products
             WHERE id IN (SELECT id FROM JSON_TABLE(?, '$[*]' COLUMNS(id CHAR(64) PATH '$')) ids)",
        )
        .bind(json!(deleted).to_string())
        .execute(&mut *conn)
        .await?;
    }
    Ok(!rows.is_empty())
}

pub(super) async fn publish_documents(
    search: &Search,
    index: &Index,
    documents: &[Value],
    deleted: &[&String],
) -> anyhow::Result<()> {
    if !documents.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index.add_or_replace(documents, Some("id")).await?,
            DOCUMENT_TASK_TIMEOUT,
        )
        .await?;
    }
    if !deleted.is_empty() {
        wait_for_task_with_timeout(
            &search.client,
            index.delete_documents(deleted).await?,
            DOCUMENT_TASK_TIMEOUT,
        )
        .await?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct CanonicalHit {
    id: String,
    options: Option<Vec<String>>,
}

pub(super) async fn hits(
    search: &Search,
    db: &MySqlPool,
    query: &str,
    allow_hint: bool,
) -> Result<(Vec<CanonicalHit>, Option<String>), MeiliError> {
    let filters = offer_filters(query);
    let original = query;
    let query = filters.terms;
    let lexical = lexical_search(search, &query).await?;
    let strong = lexical
        .hits
        .first()
        .is_some_and(|hit| hit.ranking_score.is_some_and(|score| score >= 0.85));
    if !strong {
        let option = keywords::normalize(original);
        let matches: Result<Vec<String>, _> = sqlx::query_scalar(
            "SELECT DISTINCT canonical_id FROM search_catalog_sources
             WHERE keyword_id=? AND variant_id>0 ORDER BY canonical_id LIMIT 12",
        )
        .bind(keywords::id(&option))
        .fetch_all(db)
        .await;
        match matches {
            Ok(groups) if !groups.is_empty() => {
                return Ok((
                    groups
                        .into_iter()
                        .map(|id| CanonicalHit {
                            id,
                            options: Some(vec![option.clone()]),
                        })
                        .collect(),
                    None,
                ));
            }
            Err(error) => tracing::debug!(%error, "could not check exact variant keyword"),
            _ => {}
        }
    }
    let hint = if !strong && allow_hint {
        keywords::spelling_hint(search, original, &query).await
    } else {
        None
    };
    if let Some(hint) = hint
        && let Ok(corrected) = lexical_search(search, &offer_filters(&hint).terms).await
        && !corrected.hits.is_empty()
    {
        return Ok((
            corrected.hits.into_iter().map(|hit| hit.result).collect(),
            Some(hint),
        ));
    }
    if search.hybrid
        && !strong
        && let Some(vector) = search.embedder.query(&query).await
    {
        let mut hybrid = search.canonical.search();
        hybrid
            .with_query(&query)
            .with_limit(12)
            .with_attributes_to_retrieve(Selectors::Some(&["id", "options"]))
            .with_hybrid(embeddings::MODEL, 0.55)
            .with_vector(&vector)
            .with_ranking_score_threshold(0.55);
        match tokio::time::timeout(Duration::from_millis(750), hybrid.execute::<CanonicalHit>())
            .await
        {
            Ok(Ok(result)) if !result.hits.is_empty() => {
                return Ok((
                    result.hits.into_iter().map(|hit| hit.result).collect(),
                    None,
                ));
            }
            Ok(Err(error)) => tracing::debug!(%error, "canonical hybrid fell back to keyword"),
            _ => {}
        }
    }
    Ok((
        lexical.hits.into_iter().map(|hit| hit.result).collect(),
        None,
    ))
}

async fn lexical_search(
    search: &Search,
    query: &str,
) -> Result<meilisearch_sdk::search::SearchResults<CanonicalHit>, MeiliError> {
    let mut request = search.canonical.search();
    request
        .with_query(query)
        .with_limit(12)
        .with_matching_strategy(MatchingStrategies::ALL)
        .with_attributes_to_retrieve(Selectors::Some(&["id", "options"]))
        .with_show_ranking_score(true);
    request.execute::<CanonicalHit>().await
}

pub(super) async fn offers(
    db: &MySqlPool,
    hits: Vec<CanonicalHit>,
    query: &str,
) -> anyhow::Result<Vec<Value>> {
    if hits.is_empty() {
        return Ok(Vec::new());
    }
    let term = keywords::normalize(query);
    let keys: Vec<_> = hits
        .into_iter()
        .map(|hit| {
            let option = hit
                .options
                .as_ref()
                .filter(|options| options.contains(&term))
                .map(|_| keywords::id(&term));
            json!({"id":hit.id,"option":option})
        })
        .collect();
    eligible_offers(db, &keys, query, true).await
}

pub(super) async fn listing_offers(
    db: &MySqlPool,
    ids: &[i64],
    query: &str,
) -> anyhow::Result<Vec<Value>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let keys: Vec<_> = ids.iter().map(|id| json!({"id":id.to_string()})).collect();
    eligible_offers(db, &keys, query, false).await
}

async fn eligible_offers(
    db: &MySqlPool,
    keys: &[Value],
    query: &str,
    canonical: bool,
) -> anyhow::Result<Vec<Value>> {
    let OfferFilters {
        storage, condition, ..
    } = offer_filters(query);
    let group_join = if canonical {
        "JOIN search_catalog_sources src ON src.canonical_id=(CONVERT(h.id USING ascii) COLLATE ascii_bin) AND src.variant_id=0
         JOIN products p ON p.id=src.product_id AND p.status='active'"
    } else {
        "JOIN products p ON p.id=CAST(h.id AS SIGNED) AND p.status='active'"
    };
    let mut statement = QueryBuilder::<MySql>::new(
        "WITH eligible AS (
            SELECT h.position,p.id,p.name,v.price,
              ROW_NUMBER() OVER (PARTITION BY h.position ORDER BY v.stock > 0 DESC,v.price,p.id,v.id) choice
            FROM JSON_TABLE(",
    );
    statement.push_bind(json!(keys).to_string()).push(
        ", '$[*]' COLUMNS(position FOR ORDINALITY, id CHAR(64) PATH '$.id', option_key CHAR(64) PATH '$.option')) h ",
    ).push(group_join).push(
        " JOIN shops s ON s.id=p.shop_id AND NOT s.suspended
            JOIN variants v ON v.product_id=p.id AND v.enabled
            WHERE (h.option_key IS NULL OR EXISTS (
              SELECT 1 FROM search_catalog_sources opt WHERE opt.product_id=p.id AND opt.variant_id=v.id
              AND opt.keyword_id=(CONVERT(h.option_key USING ascii) COLLATE ascii_bin)
            ))",
    );
    for (pattern, attribute) in [
        (
            storage.as_deref(),
            "(^|[^[:alnum:]])[0-9]+[[:space:]]*(gb|tb)([^[:alnum:]]|$)",
        ),
        (
            condition,
            "(^|[^[:alnum:]])(bekas|second|used|preowned|baru|new)([^[:alnum:]]|$)",
        ),
    ] {
        if let Some(pattern) = pattern {
            // Variant attributes take precedence over general wording in the merchant title.
            statement
                .push(" AND CASE WHEN LOWER(CONCAT_WS(' ',v.name,v.subvariant)) REGEXP ")
                .push_bind(attribute)
                .push(" THEN LOWER(CONCAT_WS(' ',v.name,v.subvariant)) ELSE LOWER(p.name) END REGEXP ")
                .push_bind(pattern);
        }
    }
    statement.push(") SELECT id,name,price FROM eligible WHERE choice=1 ORDER BY position LIMIT 6");
    let rows: Vec<(i64, String, i64)> = statement.build_query_as().fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, price)| json!({"id":id,"name":name,"lowest_price":price}))
        .collect())
}

pub(super) struct OfferFilters {
    pub(super) terms: String,
    storage: Option<String>,
    condition: Option<&'static str>,
}

impl OfferFilters {
    pub(super) fn constrained(&self) -> bool {
        self.storage.is_some() || self.condition.is_some()
    }
}

pub(super) fn offer_filters(query: &str) -> OfferFilters {
    let normalized = keywords::normalize(query);
    let mut words = normalized.split_whitespace().peekable();
    let mut storage = None;
    let mut state = None;
    let mut terms = Vec::new();
    while let Some(word) = words.next() {
        if let Some(number) = word
            .strip_suffix("gb")
            .or_else(|| word.strip_suffix("tb"))
            .filter(|_| is_storage(word))
        {
            storage = Some(format!(
                r"(^|[^[:alnum:]]){number}[[:space:]]*{}([^[:alnum:]]|$)",
                &word[number.len()..]
            ));
        } else if word.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(words.peek(), Some(&"gb" | &"tb"))
        {
            let unit = words.next().unwrap_or_default();
            storage = Some(format!(
                r"(^|[^[:alnum:]]){word}[[:space:]]*{unit}([^[:alnum:]]|$)"
            ));
        } else if !terms.is_empty()
            && words.peek().is_none()
            && let Some(condition) = condition(word)
        {
            state = Some(match condition {
                "used" => "(^|[^[:alnum:]])(bekas|second|used|preowned)([^[:alnum:]]|$)",
                _ => "(^|[^[:alnum:]])(baru|new)([^[:alnum:]]|$)",
            });
        } else {
            terms.push(word);
        }
    }
    let terms = if terms.is_empty() {
        query.to_owned()
    } else {
        terms.join(" ")
    };
    OfferFilters {
        terms,
        storage,
        condition: state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_preserves_models_and_accessories() {
        assert!(safe_title(
            "iphone 15 pro max 256 gb bekas",
            "iphone 15 pro max",
            false
        ));
        assert!(!safe_title("iphone 15 pro max", "iphone pro max", false));
        assert!(!safe_title(
            "case iphone 15 pro max",
            "iphone 15 pro max",
            false
        ));
        assert!(!safe_title("black 256gb", "black", true));
        assert!(safe_title("warna ocean blue", "ocean blue", true));
        assert!(!safe_title("new balance shoes", "balance shoes", false));
        assert_eq!(
            iphone_family("iphone 15 pro max").as_deref(),
            Some("iphone pro max")
        );
        let filters = offer_filters("ipon 15 pro max 256 GB second");
        assert_eq!(filters.terms, "ipon 15 pro max");
        assert!(filters.storage.unwrap().contains("256[[:space:]]*gb"));
        assert!(filters.condition.unwrap().contains("bekas"));
        assert!(!offer_filters("new balance shoes").constrained());
    }

    #[test]
    fn extracts_compact_keywords_without_changing_product_identity() {
        let title = "men s running shorts with phone pockets";
        let extracted = catalog_text(
            title,
            Entities {
                product: Some("phone".to_owned()),
                product_type: Some("running shorts".to_owned()),
                ..Entities::default()
            },
            false,
        );
        assert_eq!(extracted.name, title);
        assert_eq!(extracted.suggestion.as_deref(), Some("running shorts"));

        let case = catalog_text(
            "casing untuk iphone 15 pro max",
            Entities {
                product: Some("iphone 15 pro max".to_owned()),
                product_type: Some("casing".to_owned()),
                compatibility: Some("iphone 15 pro max".to_owned()),
                ..Entities::default()
            },
            false,
        );
        assert_eq!(case.suggestion.as_deref(), Some("casing"));
        assert_eq!(case.name, "casing untuk iphone 15 pro max");
        let unlabeled_case = catalog_text(
            "casing iphone 15 pro max",
            Entities {
                product: Some("iphone 15 pro max".to_owned()),
                ..Entities::default()
            },
            false,
        );
        assert_eq!(unlabeled_case.suggestion.as_deref(), Some("casing"));
    }
}
