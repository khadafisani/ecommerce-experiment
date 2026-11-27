use std::collections::{BTreeSet, HashMap};

use meilisearch_sdk::{
    errors::{Error as MeiliError, ErrorCode},
    indexes::Index,
    search::{MatchRange, MatchingStrategies, Selectors},
    settings::{MinWordSizeForTypos, Settings, TypoToleranceSettings},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{MySqlConnection, MySqlPool};

use super::{
    Search, canonical,
    extractor::{Extractor, Task},
    settings_match, wait_for_task,
};
use crate::json;

pub(super) const INDEX: &str = "tosca_market_keywords_v1";
const MAX_ALIASES: usize = 5;
const MIN_ALIAS_SEARCHES: u64 = 3;
const MAX_SUGGESTION_CHARS: usize = 72;
const MAX_SUGGESTION_WORDS: usize = 8;

pub(super) fn settings() -> Settings {
    Settings::new()
        .with_searchable_attributes(["phrase", "aliases"])
        .with_filterable_attributes(["id", "aliases"])
        .with_typo_tolerance(TypoToleranceSettings {
            enabled: Some(true),
            disable_on_attributes: Some(Vec::new()),
            disable_on_words: Some(Vec::new()),
            min_word_size_for_typos: Some(MinWordSizeForTypos {
                one_typo: Some(3),
                two_typos: Some(4),
            }),
            disable_on_numbers: Some(true),
        })
        .with_ranking_rules([
            "words",
            "typo",
            "proximity",
            "attribute",
            "sort",
            "exactness",
            "demand:desc",
        ])
}

pub(super) async fn ensure_index(
    search: &Search,
    index: &Index,
    settings: Settings,
) -> anyhow::Result<bool> {
    let created = match search.client.get_index(&index.uid).await {
        Ok(_) => false,
        Err(MeiliError::Meilisearch(error)) if error.error_code == ErrorCode::IndexNotFound => {
            wait_for_task(
                &search.client,
                search.client.create_index(&index.uid, Some("id")).await?,
            )
            .await?;
            true
        }
        Err(error) => return Err(error.into()),
    };
    if created || !settings_match(index, &settings).await? {
        wait_for_task(&search.client, index.set_settings(&settings).await?).await?;
    }
    Ok(created)
}

pub(super) fn normalize(value: &str) -> String {
    let lowered: String = value
        .chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    lowered.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn id(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn digit_runs(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
}

pub(super) async fn record(db: &MySqlPool, value: &str) -> anyhow::Result<()> {
    let phrase = normalize(value);
    if phrase.chars().count() < 2
        || phrase.chars().count() > 150
        || !phrase.chars().any(char::is_alphabetic)
        || value.contains('@')
        || value.contains("://")
        || digit_runs(&phrase).any(|n| n.len() >= 8)
    {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO search_query_cleanup(id,phrase) VALUES (?,?)
         ON DUPLICATE KEY UPDATE submissions=submissions+1,ready=TRUE",
    )
    .bind(id(&phrase))
    .bind(phrase)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub(super) struct Hit {
    pub(super) phrase: String,
}

pub(super) fn compact_phrase(phrase: &str) -> bool {
    !phrase.is_empty()
        && phrase.chars().count() <= MAX_SUGGESTION_CHARS
        && phrase.split_whitespace().count() <= MAX_SUGGESTION_WORDS
}

pub(super) fn leading_match(
    positions: Option<&HashMap<String, Vec<MatchRange>>>,
    attributes: &[&str],
) -> bool {
    positions.is_some_and(|positions| {
        attributes.iter().any(|attribute| {
            positions
                .get(*attribute)
                .is_some_and(|matches| matches.iter().any(|range| range.start == 0))
        })
    })
}

pub(super) async fn suggest(search: &Search, query: &str) -> Vec<Hit> {
    const FIELDS: &[&str] = &["phrase", "aliases"];
    let mut request = search.keywords.search();
    request
        .with_query(query)
        .with_limit(24)
        .with_matching_strategy(MatchingStrategies::ALL)
        .with_attributes_to_retrieve(Selectors::Some(FIELDS))
        .with_show_matches_position(true);
    match request.execute::<Hit>().await {
        Ok(result) => result
            .hits
            .into_iter()
            .filter(|hit| {
                compact_phrase(&hit.result.phrase)
                    && leading_match(hit.matches_position.as_ref(), FIELDS)
            })
            .take(6)
            .map(|hit| hit.result)
            .collect(),
        Err(error) => {
            tracing::debug!(%error, "keyword suggestions unavailable");
            Vec::new()
        }
    }
}

pub(super) async fn correction(search: &Search, query: &str) -> Option<String> {
    let normalized = normalize(query);
    if normalized.is_empty() {
        return None;
    }
    let filter = format!(
        "id = {} OR aliases = {}",
        json!(id(&normalized)),
        json!(normalized)
    );
    let mut request = search.keywords.search();
    request.with_filter(&filter).with_limit(2);
    let hits = request.execute::<Hit>().await.ok()?.hits;
    match hits.as_slice() {
        [hit] if hit.result.phrase != normalized && compact_phrase(&hit.result.phrase) => {
            Some(hit.result.phrase.clone())
        }
        _ => None,
    }
}

async fn candidates(search: &Search, value: &str) -> Vec<String> {
    let mut results = catalog_matches(search, value).await;
    if results
        .iter()
        .any(|candidate| candidate == value || plausible_typo(value, candidate))
    {
        return results;
    }
    let words: Vec<_> = value.split_whitespace().collect();
    if let [first, _, rest @ ..] = words.as_slice()
        && let Some(last) = rest.last()
        && first.chars().count() >= 4
        && last.chars().count() >= 3
    {
        results.extend(catalog_matches(search, &format!("{first} {last}")).await);
        results.sort_unstable();
        results.dedup();
    }
    results
}

async fn catalog_matches(search: &Search, value: &str) -> Vec<String> {
    let mut request = search.keywords.search();
    request
        .with_query(value)
        .with_limit(18)
        .with_matching_strategy(MatchingStrategies::ALL)
        .with_attributes_to_search_on(&["phrase"])
        .with_attributes_to_retrieve(Selectors::Some(&["phrase"]));
    match request.execute::<Hit>().await {
        Ok(result) => result
            .hits
            .into_iter()
            .map(|hit| hit.result.phrase)
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn dictionary_correction(raw: &str, candidates: &[String]) -> Option<String> {
    let words = raw.split_whitespace().count();
    let prefixes: BTreeSet<_> = candidates
        .iter()
        .filter(|phrase| compact_phrase(phrase))
        .map(|phrase| {
            phrase
                .split_whitespace()
                .take(words)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    if prefixes.contains(raw) {
        return None;
    }
    let mut plausible = prefixes
        .into_iter()
        .filter(|candidate| plausible_typo(raw, candidate));
    let first = plausible.next()?;
    plausible.next().is_none().then_some(first)
}

pub(super) async fn spelling_hint(search: &Search, original: &str, core: &str) -> Option<String> {
    let original = normalize(original);
    let core = normalize(core);
    if core.is_empty() {
        return None;
    }
    let corrected = dictionary_correction(&core, &candidates(search, &core).await)?;
    restore_query_details(&original, &core, &corrected)
}

fn restore_query_details(original: &str, core: &str, corrected: &str) -> Option<String> {
    let mut terms = core
        .split_whitespace()
        .zip(corrected.split_whitespace())
        .peekable();
    let result: Vec<_> = original
        .split_whitespace()
        .map(|word| {
            if terms.peek().is_some_and(|(source, _)| *source == word) {
                terms.next().map_or(word, |(_, replacement)| replacement)
            } else {
                word
            }
        })
        .collect();
    terms.peek().is_none().then(|| result.join(" "))
}

#[derive(sqlx::FromRow)]
struct QueryJob {
    id: String,
    phrase: String,
}

#[derive(Deserialize, Serialize)]
struct Alias {
    phrase: String,
    submissions: u64,
}

pub(super) async fn clean_queries(
    conn: &mut MySqlConnection,
    extractor: &mut Extractor,
    search: &Search,
) -> anyhow::Result<()> {
    sqlx::query(
        "DELETE FROM search_query_cleanup WHERE created_at < CURRENT_TIMESTAMP(6)-INTERVAL 7 DAY
         ORDER BY created_at,id LIMIT 2000",
    )
    .execute(&mut *conn)
    .await?;
    let jobs: Vec<QueryJob> = sqlx::query_as(
        "SELECT id,phrase FROM search_query_cleanup
         WHERE ready ORDER BY created_at,id LIMIT 12",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut batch = Vec::new();
    for job in jobs {
        if active_keyword(conn, &job.id).await? {
            apply_query(conn, &job, Some(&job.id)).await?;
            continue;
        }
        let suggestion = catalog_hint(search, &job.phrase).await;
        if let Some(key) = active_suggestion(conn, suggestion.as_deref()).await? {
            apply_query(conn, &job, Some(&key)).await?;
            continue;
        }
        batch.push(job);
    }
    if batch.is_empty() {
        return Ok(());
    }
    let inputs: Vec<_> = batch.iter().map(|job| job.phrase.clone()).collect();
    let results = extractor.extract(Task::Query, &inputs).await?;
    for (job, entities) in batch.iter().zip(results) {
        let core = entities
            .product
            .or(entities.product_type)
            .or(entities.brand)
            .map(|span| normalize(&span))
            .filter(|core| !core.is_empty() && *core != job.phrase && job.phrase.contains(core));
        let suggestion = match core {
            Some(core) => spelling_hint(search, &job.phrase, &core).await,
            None => None,
        };
        let corrected_id = active_suggestion(conn, suggestion.as_deref()).await?;
        apply_query(conn, job, corrected_id.as_deref()).await?;
    }
    Ok(())
}

async fn catalog_hint(search: &Search, phrase: &str) -> Option<String> {
    if let Some(alias) = correction(search, phrase).await {
        return Some(alias);
    }
    if let Some(hint) = spelling_hint(search, phrase, phrase).await {
        return Some(hint);
    }
    let filters = canonical::offer_filters(phrase);
    if filters.constrained() && filters.terms != phrase {
        return spelling_hint(search, phrase, &filters.terms).await;
    }
    None
}

async fn active_suggestion(
    conn: &mut MySqlConnection,
    phrase: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let Some(phrase) = phrase else {
        return Ok(None);
    };
    let key = id(phrase);
    Ok(active_keyword(conn, &key).await?.then_some(key))
}

async fn active_keyword(conn: &mut MySqlConnection, key: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM search_catalog_sources WHERE keyword_id=?)",
        )
        .bind(key)
        .fetch_one(conn)
        .await?,
    )
}

async fn apply_query(
    conn: &mut MySqlConnection,
    job: &QueryJob,
    canonical: Option<&str>,
) -> anyhow::Result<()> {
    use sqlx::Connection;

    let mut tx = conn.begin().await?;
    let (submissions, counted): (u64, u64) = sqlx::query_as(
        "SELECT submissions,counted FROM search_query_cleanup WHERE id=? FOR UPDATE",
    )
    .bind(&job.id)
    .fetch_one(&mut *tx)
    .await?;
    let mut remove = false;
    if let Some(canonical) = canonical {
        let sqlx::types::Json(mut aliases): sqlx::types::Json<Vec<Alias>> =
            sqlx::query_scalar("SELECT aliases FROM search_keyword_terms WHERE id=? FOR UPDATE")
                .bind(canonical)
                .fetch_one(&mut *tx)
                .await?;
        if canonical == job.id {
            remove = true;
        } else if let Some(existing) = aliases.iter_mut().find(|alias| alias.phrase == job.phrase) {
            existing.submissions += submissions - counted;
            remove = true;
        } else if submissions >= MIN_ALIAS_SEARCHES {
            aliases.push(Alias {
                phrase: job.phrase.clone(),
                submissions,
            });
            aliases.sort_by_key(|alias| std::cmp::Reverse(alias.submissions));
            aliases.truncate(MAX_ALIASES);
            remove = aliases.iter().any(|alias| alias.phrase == job.phrase);
        }
        sqlx::query(
            "UPDATE search_keyword_terms SET demand=demand+?,aliases=?,dirty=TRUE WHERE id=?",
        )
        .bind(submissions - counted)
        .bind(sqlx::types::Json(aliases))
        .bind(canonical)
        .execute(&mut *tx)
        .await?;
    }
    if remove {
        sqlx::query("DELETE FROM search_query_cleanup WHERE id=?")
            .bind(&job.id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("UPDATE search_query_cleanup SET ready=FALSE,counted=IF(?,submissions,counted) WHERE id=?")
            .bind(canonical.is_some())
            .bind(&job.id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn plausible_typo(raw: &str, cleaned: &str) -> bool {
    if raw == cleaned
        || raw.split_whitespace().count() != cleaned.split_whitespace().count()
        || raw
            .split_whitespace()
            .zip(cleaned.split_whitespace())
            .any(|(a, b)| {
                a != b
                    && (a.bytes().any(|byte| byte.is_ascii_digit())
                        || b.bytes().any(|byte| byte.is_ascii_digit()))
            })
    {
        return false;
    }
    let left: Vec<_> = raw.chars().collect();
    let right: Vec<_> = cleaned.chars().collect();
    let budget = if left.len() == 4 && right.first().is_some_and(|&first| raw.starts_with(first)) {
        2
    } else {
        (left.len() / 4).clamp(1, 4)
    };
    if left.len().abs_diff(right.len()) > budget {
        return false;
    }
    let mut earlier = vec![0; right.len() + 1];
    let mut previous: Vec<_> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (i, a) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, b) in right.iter().enumerate() {
            current[j + 1] = (previous[j] + usize::from(a != b))
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
            if i > 0 && j > 0 && left[i - 1] == *b && right[j - 1] == *a {
                current[j + 1] = current[j + 1].min(earlier[j - 1] + 1);
            }
        }
        std::mem::swap(&mut earlier, &mut previous);
        std::mem::swap(&mut current, &mut previous);
    }
    previous[right.len()] <= budget
}

#[derive(sqlx::FromRow)]
struct KeywordDocument {
    id: String,
    phrase: String,
    demand: u64,
    aliases: sqlx::types::Json<Vec<Alias>>,
    active: bool,
}

pub(super) async fn publish(conn: &mut MySqlConnection, search: &Search) -> anyhow::Result<bool> {
    let rows: Vec<KeywordDocument> = sqlx::query_as(
        "SELECT k.id,k.phrase,k.demand,k.aliases,
         EXISTS(SELECT 1 FROM search_catalog_sources s WHERE s.keyword_id=k.id) active
         FROM search_keyword_terms k WHERE dirty ORDER BY id LIMIT 256",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut documents = Vec::new();
    let mut deleted = Vec::new();
    let mut ids = Vec::new();
    for row in &rows {
        ids.push(&row.id);
        if row.active && compact_phrase(&row.phrase) {
            let aliases: Vec<_> = row.aliases.0.iter().map(|alias| &alias.phrase).collect();
            documents.push(
                json!({"id":row.id,"phrase":row.phrase,"aliases":aliases,"demand":row.demand}),
            );
        } else {
            deleted.push(&row.id);
        }
    }
    super::canonical::publish_documents(search, &search.keywords, &documents, &deleted).await?;
    if !ids.is_empty() {
        sqlx::query(
            "UPDATE search_keyword_terms SET dirty=FALSE
             WHERE id IN (SELECT id FROM JSON_TABLE(?, '$[*]' COLUMNS(id CHAR(64) PATH '$')) ids)",
        )
        .bind(json!(ids).to_string())
        .execute(conn)
        .await?;
    }
    Ok(!rows.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typo_correction_preserves_product_details() {
        assert!(plausible_typo("ipon pro max", "iphone pro max"));
        assert!(plausible_typo("ipghone rpo max", "iphone pro max"));
        assert!(!plausible_typo("ipon 15 pro max", "iphone 16 pro max"));
        assert!(!plausible_typo("case ipon 15", "iphone 15"));
        assert_eq!(normalize("  IPHONE–15  Pro MAX "), "iphone 15 pro max");
        let catalog = [
            "iphone 15 pro max",
            "iphone 16 pro max",
            "iphone 15 pro max 512gb bekas",
        ]
        .map(str::to_owned);
        let corrected = dictionary_correction("ipon 15 pro max", &catalog).unwrap();
        assert_eq!(corrected, "iphone 15 pro max");
        assert_eq!(
            restore_query_details(
                "ipon 15 pro max 256 gb bekas",
                "ipon 15 pro max",
                &corrected
            )
            .as_deref(),
            Some("iphone 15 pro max 256 gb bekas")
        );
        assert_eq!(
            dictionary_correction("ipon", &catalog).as_deref(),
            Some("iphone")
        );
        assert_eq!(dictionary_correction("iphone 15 pro max", &catalog), None);
        assert_eq!(
            dictionary_correction("iphnoe", &catalog).as_deref(),
            Some("iphone")
        );
        assert!(!plausible_typo("s23 samsung", "s23fe samsung"));
    }
    #[test]
    fn autocomplete_rejects_long_titles_and_incidental_matches() {
        assert!(compact_phrase("iphone 15 pro max 256gb"));
        assert!(!compact_phrase(
            "men s 2 in 1 running shorts 5 quick dry gym athletic workout shorts for men with phone pockets"
        ));
        assert!(!compact_phrase("a b c d e f g h i"));

        let incidental =
            serde_json::from_value(json!({"phrase":[{"start":81,"length":5}]})).unwrap();
        assert!(!leading_match(Some(&incidental), &["phrase", "aliases"]));
        let alias =
            serde_json::from_value(json!({"aliases":[{"start":0,"length":4,"indices":[0]}]}))
                .unwrap();
        assert!(leading_match(Some(&alias), &["phrase", "aliases"]));
    }
}
