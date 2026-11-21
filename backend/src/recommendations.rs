#[cfg(test)]
mod benchmark;
mod cache;

use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, State},
    http::header::CACHE_CONTROL,
    response::{IntoResponse, Response},
    routing::post,
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::QueryBuilder;
use time::OffsetDateTime;

use crate::{
    App, Error, HeaderMap, Json, MySql, Result, Router, Transaction, Value, auth::User, json, ok,
    setting,
};

const DAY: i64 = 86_400;
const CANDIDATES: usize = 96;
const EXPLORE_PAGE: usize = 20;
const TTL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct Recommendations {
    cache: Option<Arc<cache::Cache>>,
}

impl Recommendations {
    pub fn new() -> Self {
        let cache = (setting("RECOMMENDATIONS_ENABLED", "false") == "true")
            .then(|| Arc::new(cache::Cache::default()));
        Self { cache }
    }

    pub fn enabled(&self) -> bool {
        self.cache.is_some()
    }
}

pub fn routes() -> Router<App> {
    Router::new()
        .route("/recommendations", post(recommendations))
        .route("/recommendations/explore", post(explore))
        .route("/recommendation-events/view", post(view))
        .layer(DefaultBodyLimit::max(16 * 1024))
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum SignalKind {
    View,
    Cart,
    #[serde(skip_deserializing)]
    Checkout,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct Signal {
    product_id: i64,
    kind: SignalKind,
    at: i64,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Sort {
    #[default]
    Relevance,
    Newest,
    PriceAsc,
    PriceDesc,
    #[serde(skip_deserializing)]
    Explore,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct Filters {
    product_id: Option<i64>,
    category_id: Option<i64>,
    shop_id: Option<i64>,
    min_price: Option<i64>,
    max_price: Option<i64>,
    #[serde(default)]
    sort: Sort,
}

#[derive(Deserialize)]
struct Request {
    #[serde(flatten)]
    filters: Filters,
    #[serde(default)]
    signals: Vec<Signal>,
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct View {
    product_id: i64,
}

async fn view(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<View>,
) -> Result {
    if !app.recommendations.enabled() {
        return Ok(ok());
    }
    if input.product_id <= 0 {
        return Err(Error::bad("Invalid product"));
    }
    if let Some(user) = User::optional(&headers, &app)
        .await?
        .filter(|user| !user.is_guest())
    {
        sqlx::query(
            "INSERT IGNORE INTO recommendation_signals(user_id,product_id,kind,signal_day,seen_at)
             SELECT ?,p.id,'view',UTC_DATE(),UTC_TIMESTAMP()
             FROM products p JOIN shops s ON s.id=p.shop_id
             WHERE p.id=? AND p.status='active' AND NOT s.suspended",
        )
        .bind(user.id)
        .bind(input.product_id)
        .execute(&app.db)
        .await?;
    } else {
        crate::auth::rate_limit(&app.db, "guest-view", peer.ip(), 60).await?;
        sqlx::query(
            "INSERT INTO recommendation_guest_views(view_hour,product_id)
             SELECT DATE_FORMAT(UTC_TIMESTAMP(),'%Y-%m-%d %H:00:00'),p.id
             FROM products p JOIN shops s ON s.id=p.shop_id
             WHERE p.id=? AND p.status='active' AND NOT s.suspended
             ON DUPLICATE KEY UPDATE views=views+1",
        )
        .bind(input.product_id)
        .execute(&app.db)
        .await?;
    }
    Ok(ok())
}

pub async fn record_cart(
    tx: &mut Transaction<'_, MySql>,
    user_id: i64,
    product_id: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO recommendation_signals(user_id,product_id,kind,signal_day,seen_at)
         VALUES (?,?,'cart',UTC_DATE(),UTC_TIMESTAMP()) ON DUPLICATE KEY UPDATE seen_at=seen_at",
    )
    .bind(user_id)
    .bind(product_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn recommendations(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<Request>,
) -> Result<Response> {
    if !app.recommendations.enabled() {
        return Ok(reply(false, Vec::new()));
    }
    let limit = input.limit.unwrap_or(8);
    if !(1..=24).contains(&limit)
        || input.signals.len() > 100
        || [
            input.filters.product_id,
            input.filters.category_id,
            input.filters.shop_id,
        ]
        .into_iter()
        .flatten()
        .any(|id| id <= 0)
        || [input.filters.min_price, input.filters.max_price]
            .into_iter()
            .flatten()
            .any(|price| price < 0)
        || matches!((input.filters.min_price, input.filters.max_price), (Some(a), Some(b)) if a > b)
    {
        return Err(Error::bad("Invalid recommendation filters"));
    }
    let user = User::optional(&headers, &app).await?;
    let ranking = candidate_ids(&app, user.as_ref(), input.filters.clone(), input.signals).await;
    let mut items = ranked_cards(&app.db, &input.filters, &ranking, limit).await?;
    if items.len() < limit {
        let seen: Vec<_> = items
            .iter()
            .filter_map(|card| card["id"].as_i64())
            .collect();
        items.extend(live_cards(&app.db, &input.filters, None, &seen, limit - items.len()).await?);
    }
    if matches!(input.filters.sort, Sort::PriceAsc | Sort::PriceDesc) {
        items.sort_by(|a, b| {
            let price = a["lowest_price"].as_i64().cmp(&b["lowest_price"].as_i64());
            let price = if matches!(input.filters.sort, Sort::PriceDesc) {
                price.reverse()
            } else {
                price
            };
            price.then_with(|| b["id"].as_i64().cmp(&a["id"].as_i64()))
        });
    }
    items.truncate(limit);
    Ok(reply(true, items))
}

async fn candidate_ids(
    app: &App,
    user: Option<&User>,
    filters: Filters,
    mut signals: Vec<Signal>,
) -> Vec<i64> {
    let Some(cache) = &app.recommendations.cache else {
        return Vec::new();
    };
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if user.is_some_and(|user| !user.is_guest()) {
        signals.clear();
    } else {
        normalize_guest_signals(&mut signals, now);
    }
    let ranked = async {
        let profile = fetch_profile(app, user, signals).await?;
        // A new view must produce a new ranking key even when another pod handled it.
        let key = crate::auth::digest(&serde_json::to_string(&(
            user.map(|user| user.id),
            &filters,
            &profile,
        ))?);
        let anchors = cache::read_products(
            &app.db,
            profile
                .iter()
                .map(|signal| signal.product_id)
                .chain(filters.product_id)
                .collect(),
        )
        .await?;
        cache.rank(key, filters, profile, now, anchors).await
    };
    ranked
        .await
        .inspect_err(|error| tracing::warn!(%error, "recommendation ranking unavailable"))
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct ExploreRequest {
    #[serde(default)]
    signals: Vec<Signal>,
    cursor: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct ExploreCursor {
    kind: String,
    viewer: Option<i64>,
    exp: i64,
    pending: Vec<i64>,
    shown: Vec<i64>,
    before: Option<i64>,
}

async fn explore(
    State(app): State<App>,
    headers: HeaderMap,
    Json(input): Json<ExploreRequest>,
) -> Result<Response> {
    if !app.recommendations.enabled() {
        return Ok(private_json(
            json!({"enabled": false, "items": [], "cursor": null}),
        ));
    }
    if input.signals.len() > 100
        || input
            .cursor
            .as_ref()
            .is_some_and(|cursor| cursor.len() > 4_000)
    {
        return Err(Error::bad("Invalid Explore products request"));
    }
    let user = User::optional(&headers, &app).await?;
    let viewer = user.as_ref().map(|user| user.id);
    let filters = Filters {
        sort: Sort::Explore,
        ..Filters::default()
    };
    let mut cursor = if let Some(token) = input.cursor {
        let data = jsonwebtoken::decode::<ExploreCursor>(
            &token,
            &DecodingKey::from_secret(app.secret.as_bytes()).map_err(Error::internal)?,
            &Validation::default(),
        )
        .map_err(|_| Error::bad("Explore products expired. Reload the list."))?
        .claims;
        if data.kind != "explore" || data.viewer != viewer {
            return Err(Error::bad("Invalid Explore products cursor"));
        }
        data
    } else {
        ExploreCursor {
            kind: "explore".into(),
            viewer,
            exp: OffsetDateTime::now_utc().unix_timestamp() + DAY,
            pending: candidate_ids(&app, user.as_ref(), filters.clone(), input.signals).await,
            shown: Vec::new(),
            before: None,
        }
    };
    let mut items = Vec::new();
    while items.len() < EXPLORE_PAGE && !cursor.pending.is_empty() {
        let take = (EXPLORE_PAGE - items.len()).min(cursor.pending.len());
        let batch: Vec<_> = cursor.pending.drain(..take).collect();
        let cards = ranked_cards(&app.db, &filters, &batch, take).await?;
        cursor
            .shown
            .extend(cards.iter().filter_map(|item| item["id"].as_i64()));
        items.extend(cards);
    }
    let mut more = true;
    if cursor.pending.is_empty() {
        let remaining = EXPLORE_PAGE - items.len();
        let mut tail = crate::catalog::explore_tail(
            &app.db,
            cursor.before,
            &cursor.shown,
            (remaining + 1) as u32,
        )
        .await?;
        more = tail.len() > remaining;
        tail.truncate(remaining);
        if let Some(last) = tail.last() {
            cursor.before = last["id"].as_i64();
        }
        items.extend(tail);
    }
    let next = if more {
        Some(
            jsonwebtoken::encode(
                &Header::default(),
                &cursor,
                &EncodingKey::from_secret(app.secret.as_bytes()).map_err(Error::internal)?,
            )
            .map_err(Error::internal)?,
        )
    } else {
        None
    };
    Ok(private_json(
        json!({"enabled": true, "items":items, "cursor":next}),
    ))
}

async fn ranked_cards(
    db: &sqlx::MySqlPool,
    filters: &Filters,
    ids: &[i64],
    limit: usize,
) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    for batch in ids.chunks(24) {
        let mut current = live_cards(db, filters, Some(batch), &[], batch.len()).await?;
        current.sort_by_key(|card| batch.iter().position(|id| card["id"].as_i64() == Some(*id)));
        items.extend(current);
        if items.len() >= limit {
            break;
        }
    }
    Ok(items)
}

fn reply(enabled: bool, items: Vec<Value>) -> Response {
    private_json(json!({"enabled":enabled,"items":items}))
}

fn private_json(value: Value) -> Response {
    ([(CACHE_CONTROL, "private, no-store")], Json(value)).into_response()
}

fn normalize_guest_signals(signals: &mut Vec<Signal>, now: i64) {
    signals
        .retain(|signal| signal.product_id > 0 && (now - 30 * DAY..=now + 60).contains(&signal.at));
    signals
        .iter_mut()
        .for_each(|signal| signal.at = signal.at.div_euclid(DAY) * DAY);
    signals.sort_unstable_by_key(|signal| (signal.product_id, signal.kind, signal.at));
    signals.dedup_by_key(|signal| (signal.product_id, signal.kind, signal.at));
}

async fn fetch_profile(
    app: &App,
    user: Option<&User>,
    mut guest: Vec<Signal>,
) -> anyhow::Result<Vec<Signal>> {
    let Some(user) = user else {
        return Ok(guest);
    };
    let days = if user.is_guest() { 30_i32 } else { 90_i32 };
    let orders: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT l.product_id,CAST(UNIX_TIMESTAMP(c.created_at) AS SIGNED)
         FROM checkouts c JOIN orders o ON o.checkout_id=c.id JOIN order_lines l ON l.order_id=o.id
         WHERE c.user_id=? AND c.created_at>=UTC_TIMESTAMP()-INTERVAL ? DAY
           AND c.payment<>'cancelled' AND o.status NOT IN ('rejected','cancelled')
         GROUP BY c.id,c.created_at,l.product_id ORDER BY c.created_at DESC,c.id DESC,l.product_id LIMIT 600",
    )
    .bind(user.id).bind(days).fetch_all(&app.db).await?;
    if !user.is_guest() {
        let rows: Vec<(i64, String, i64)> = sqlx::query_as(
            "SELECT product_id,kind,TIMESTAMPDIFF(SECOND,'1970-01-01 00:00:00',seen_at) FROM recommendation_signals
             WHERE user_id=? AND signal_day>=UTC_DATE()-INTERVAL 90 DAY
             ORDER BY signal_day DESC,product_id,kind LIMIT 600",
        )
        .bind(user.id).fetch_all(&app.db).await?;
        guest.extend(rows.into_iter().map(|(product_id, kind, at)| Signal {
            product_id,
            at,
            kind: if kind == "view" {
                SignalKind::View
            } else {
                SignalKind::Cart
            },
        }));
    }
    guest.extend(orders.into_iter().map(|(product_id, at)| Signal {
        product_id,
        at,
        kind: SignalKind::Checkout,
    }));
    Ok(guest)
}

async fn live_cards(
    db: &sqlx::MySqlPool,
    filters: &Filters,
    ids: Option<&[i64]>,
    excluded: &[i64],
    limit: usize,
) -> Result<Vec<Value>> {
    const STOCK_PRICE: &str =
        "(SELECT MIN(v.price) FROM variants v WHERE v.product_id=p.id AND v.enabled AND v.stock>0)";
    let by_price = matches!(filters.sort, Sort::PriceAsc | Sort::PriceDesc);
    let mut query = if by_price {
        QueryBuilder::<MySql>::new(
            "SELECT JSON_OBJECT('id',p.id,'name',p.name,'image',p.image,'shop_id',s.id,'shop',s.name,
             'category',c.name,'lowest_price',MIN(v.price)) FROM products p
             JOIN shops s ON s.id=p.shop_id LEFT JOIN taxonomy c ON c.id=p.category_id
             JOIN variants v ON v.product_id=p.id AND v.enabled AND v.stock>0
             WHERE p.status='active' AND NOT s.suspended",
        )
    } else {
        let index = match (ids, filters.shop_id, filters.category_id) {
            (Some(_), _, _) => "PRIMARY",
            (_, _, Some(_)) => "product_category_status_id",
            (_, Some(_), _) => "product_shop_status_id",
            _ => "status",
        };
        crate::catalog::ordered_cards(index, format!(
            "SELECT JSON_OBJECT('id',p.id,'name',p.name,'image',p.image,'shop_id',s.id,'shop',s.name,
             'category',c.name,'lowest_price',{STOCK_PRICE})"
        ))
    };
    for (value, clause) in [
        (filters.product_id, " AND p.id<>"),
        (filters.category_id, " AND p.category_id="),
        (filters.shop_id, " AND p.shop_id="),
    ] {
        if let Some(value) = value {
            query.push(clause).push_bind(value);
        }
    }
    for (set, clause) in [
        (ids.unwrap_or(&[]), " AND p.id IN ("),
        (excluded, " AND p.id NOT IN ("),
    ] {
        if !set.is_empty() {
            query.push(clause);
            let mut params = query.separated(",");
            for id in set {
                params.push_bind(*id);
            }
            params.push_unseparated(")");
        }
    }
    if by_price {
        query.push(" GROUP BY p.id,p.name,p.image,s.id,s.name,c.name HAVING COUNT(*)>0");
        if let Some(price) = filters.min_price {
            query.push(" AND MIN(v.price)>=").push_bind(price);
        }
        if let Some(price) = filters.max_price {
            query.push(" AND MIN(v.price)<=").push_bind(price);
        }
    } else {
        query
            .push(" AND ")
            .push(STOCK_PRICE)
            .push(" BETWEEN ")
            .push_bind(filters.min_price.unwrap_or(i64::MIN))
            .push(" AND ")
            .push_bind(filters.max_price.unwrap_or(i64::MAX));
    }
    query.push(match filters.sort {
        Sort::PriceAsc => " ORDER BY MIN(v.price) ASC,p.id DESC",
        Sort::PriceDesc => " ORDER BY MIN(v.price) DESC,p.id DESC",
        _ => " ORDER BY p.id DESC",
    });
    query.push(" LIMIT ").push_bind(limit as u32);
    crate::catalog::decode_rows(query.build_query_scalar().fetch_all(db).await?)
}

pub async fn refresh_worker(app: App) {
    let Some(cache) = &app.recommendations.cache else {
        return;
    };
    loop {
        let started = std::time::Instant::now();
        let result = match tokio::time::timeout(
            Duration::from_secs(15),
            cache::read_catalog(&app.db),
        )
        .await
        {
            Ok(Ok(rows)) => {
                let candidates = rows.len();
                let result = cache.refresh(rows, started).await;
                if result.is_ok() {
                    tracing::debug!(candidates, elapsed = ?started.elapsed(), "recommendation candidates refreshed");
                }
                result
            }
            Ok(Err(error)) => Err(error),
            Err(error) => {
                Err(anyhow::Error::new(error).context("reading recommendation candidates"))
            }
        };
        match result {
            Ok(()) => {
                let jitter = u64::from(uuid::Uuid::new_v4().as_bytes()[0]) % 4;
                tokio::time::sleep_until((started + TTL - Duration::from_secs(jitter + 1)).into())
                    .await;
            }
            Err(error) => {
                tracing::warn!(%error, "recommendation refresh failed");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guest_history_is_bounded_to_recent_distinct_signals() {
        let now = 100 * DAY + 80;
        let mut signals = vec![
            Signal {
                product_id: 1,
                kind: SignalKind::View,
                at: now - 1,
            },
            Signal {
                product_id: 1,
                kind: SignalKind::View,
                at: now,
            },
            Signal {
                product_id: 1,
                kind: SignalKind::Cart,
                at: now,
            },
            Signal {
                product_id: 1,
                kind: SignalKind::View,
                at: now - 31 * DAY,
            },
            Signal {
                product_id: 2,
                kind: SignalKind::View,
                at: now + 61,
            },
        ];
        normalize_guest_signals(&mut signals, now);
        assert_eq!(signals.len(), 2);
        assert!(
            signals
                .iter()
                .all(|signal| signal.product_id == 1 && signal.at == 100 * DAY)
        );
    }
}
