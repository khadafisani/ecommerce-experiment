use super::{App, Error, Json, Result, Router, User, json, member};
use axum::{
    extract::{Path, Query, State},
    http::header::CACHE_CONTROL,
    response::{IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use sqlx::AssertSqlSafe;

pub(super) fn routes() -> Router<App> {
    Router::new()
        .route("/admin/{section}/{id}/history", get(admin_history))
        .route(
            "/merchant/{shop}/{section}/{id}/history",
            get(merchant_history),
        )
}

// Use explicit projections so private credentials and newly added columns stay out of history.
const PRODUCT: &str = "'name',h.name,'description',h.description,'image',h.image,'shop_id',h.shop_id,'brand_id',h.brand_id,'category_id',h.category_id,'status',h.status,'note',h.note,'actor_id',h.actor_id";
const VARIANT: &str = "'name',h.name,'subvariant',h.subvariant,'price',h.price,'stock',h.stock,'grams',h.grams,'enabled',h.enabled,'actor_id',h.actor_id";
const REVIEW: &str = "'product_id',h.product_id,'order_id',h.order_id,'user_id',h.user_id,'rating',h.rating,'body',h.body,'media',JSON_EXTRACT(h.media,'$'),'reply',h.reply,'deleted',h.deleted,'actor_id',h.actor_id";
const ORDER: &str = "'checkout_id',h.checkout_id,'shop_id',h.shop_id,'status',h.status,'subtotal',h.subtotal,'shipping',h.shipping,'tax',h.tax,'courier_id',h.courier_id,'receipt',h.receipt,'completed_at',h.completed_at,'actor_id',h.actor_id";
const RETURN: &str = "'description',h.description,'media',JSON_EXTRACT(h.media,'$')";
const PAYMENT: &str = "'checkout_no',h.checkout_no,'user_id',h.user_id,'address',h.address,'subtotal',h.subtotal,'tax',h.tax,'shipping',h.shipping,'total',h.total,'payment',h.payment,'paid_by',h.paid_by";

#[derive(Deserialize)]
struct HistoryQuery {
    kind: Option<String>,
    cursor: Option<String>,
}

async fn admin_history(
    State(app): State<App>,
    user: User,
    Path((section, id)): Path<(String, i64)>,
    Query(query): Query<HistoryQuery>,
) -> Result<Response> {
    user.admin()?;
    let source = match (section.as_str(), query.kind.as_deref()) {
        ("products", None) => ("products", "id", "h.id=?", PRODUCT),
        ("products", Some("variants")) => ("variants", "id", "h.product_id=?", VARIANT),
        ("products", Some("reviews")) => ("reviews", "id", "h.product_id=?", REVIEW),
        ("reviews", None) => ("reviews", "id", "h.id=?", REVIEW),
        ("taxonomy", None) => (
            "taxonomy",
            "id",
            "h.id=?",
            "'name',h.name,'kind',h.kind,'status',h.status",
        ),
        ("couriers", None) => (
            "couriers",
            "id",
            "h.id=?",
            "'name',h.name,'logo',h.logo,'url',h.url",
        ),
        ("shops", None) => (
            "shops",
            "id",
            "h.id=?",
            "'name',h.name,'suspended',h.suspended,'owner_id',h.owner_id",
        ),
        ("users", None) => (
            "users",
            "id",
            "h.id=?",
            "'name',h.name,'email',h.email,'admin',h.admin,'suspended',h.suspended,'guest',h.guest",
        ),
        ("payments", None) => ("checkouts", "id", "h.id=?", PAYMENT),
        ("payments", Some("orders")) => ("orders", "id", "h.checkout_id=?", ORDER),
        ("payments", Some("returns")) => (
            "returns",
            "order_id",
            "h.order_id IN (SELECT id FROM orders WHERE checkout_id=?)",
            RETURN,
        ),
        _ => return Err(Error::missing()),
    };
    history_page(&app, source, id, None, query).await
}

async fn merchant_history(
    State(app): State<App>,
    user: User,
    Path((shop, section, id)): Path<(i64, String, i64)>,
    Query(query): Query<HistoryQuery>,
) -> Result<Response> {
    member(&app, &user, shop, section == "team").await?;
    let source = match (section.as_str(), query.kind.as_deref()) {
        ("products", None) => ("products", "id", "h.id=? AND h.shop_id=?", PRODUCT),
        ("products", Some("variants")) => (
            "variants",
            "id",
            "h.product_id=? AND EXISTS (SELECT 1 FROM products WHERE id=h.product_id AND shop_id=?)",
            VARIANT,
        ),
        ("products", Some("reviews")) => (
            "reviews",
            "id",
            "h.product_id=? AND EXISTS (SELECT 1 FROM products WHERE id=h.product_id AND shop_id=?)",
            REVIEW,
        ),
        ("orders", None) => ("orders", "id", "h.id=? AND h.shop_id=?", ORDER),
        ("orders", Some("returns")) => (
            "returns",
            "order_id",
            "h.order_id=? AND EXISTS (SELECT 1 FROM orders WHERE id=h.order_id AND shop_id=?)",
            RETURN,
        ),
        ("team", None) => (
            "members",
            "user_id",
            "h.user_id=? AND h.shop_id=?",
            "'shop_id',h.shop_id,'active',h.active,'updated_by',h.updated_by",
        ),
        _ => return Err(Error::missing()),
    };
    history_page(&app, source, id, Some(shop), query).await
}

#[derive(sqlx::FromRow)]
struct Version {
    record_id: i64,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    snapshot: sqlx::types::Json<serde_json::Value>,
}

async fn history_page(
    app: &App,
    (table, row_id, scope, fields): (&str, &str, &str, &str),
    id: i64,
    shop: Option<i64>,
    query: HistoryQuery,
) -> Result<Response> {
    const PAGE_SIZE: usize = 20;
    let before = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let cursor_filter = if before.is_some() {
        format!(" AND (h.ROW_START<? OR (h.ROW_START=? AND h.{row_id}<?))")
    } else {
        String::new()
    };
    // All interpolated SQL comes from the allowlists above. IDs and cursors are bound.
    let sql = format!(
        "SELECT h.{row_id} AS record_id,h.ROW_START AS valid_from,
                IF(h.ROW_END>CURRENT_TIMESTAMP(6),NULL,h.ROW_END) AS valid_until,
                JSON_OBJECT({fields},'created_at',h.created_at,'updated_at',h.updated_at) AS snapshot
         FROM {table} FOR SYSTEM_TIME ALL AS h WHERE {scope}{cursor_filter}
         ORDER BY h.ROW_START DESC,h.{row_id} DESC LIMIT ?"
    );
    let mut statement = sqlx::query_as::<_, Version>(AssertSqlSafe(sql)).bind(id);
    if let Some(shop) = shop {
        statement = statement.bind(shop);
    }
    if let Some((timestamp, record)) = before {
        statement = statement.bind(timestamp).bind(timestamp).bind(record);
    }
    let mut versions = statement
        .bind((PAGE_SIZE + 1) as u32)
        .fetch_all(&app.db)
        .await?;
    if versions.is_empty() && before.is_none() && query.kind.is_none() {
        return Err(Error::missing());
    }
    let has_more = versions.len() > PAGE_SIZE;
    versions.truncate(PAGE_SIZE);
    let next = versions.last().filter(|_| has_more).map(Version::key);
    let items: Vec<_> = versions
        .into_iter()
        .map(|version| {
            json!({
                "id": version.key(),
                "record_id": version.record_id,
                "valid_from": version.valid_from,
                "valid_until": version.valid_until,
                "snapshot": version.snapshot.0,
            })
        })
        .collect();
    Ok((
        [(CACHE_CONTROL, "private, no-store")],
        Json(json!({"items":items,"cursor":next})),
    )
        .into_response())
}

impl Version {
    fn key(&self) -> String {
        format!(
            "{}|{}",
            self.valid_from.to_rfc3339_opts(SecondsFormat::Micros, true),
            self.record_id
        )
    }
}

fn parse_cursor(cursor: &str) -> Result<(DateTime<Utc>, i64)> {
    let invalid = || Error::bad("Invalid history cursor");
    let (timestamp, id) = cursor.split_once('|').ok_or_else(invalid)?;
    Ok((
        timestamp.parse().map_err(|_| invalid())?,
        id.parse().map_err(|_| invalid())?,
    ))
}
