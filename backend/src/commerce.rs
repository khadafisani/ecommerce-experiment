use super::*;
use crate::{
    auth::User,
    catalog::{self, Filter, decode_rows},
    uploads::{self, ImageSize},
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/cart", get(cart))
        .route("/cart/availability", post(availability))
        .route("/cart/{id}", put(set_cart))
        .route("/addresses", get(addresses).post(add_address))
        .route("/checkout/quote", post(quote))
        .route("/checkout", get(checkouts).post(checkout))
        .route("/checkout/{id}", get(detail))
        .route("/checkout/{id}/cancel", post(cancel))
        .route("/checkout/{id}/pay", post(pay))
        .route("/orders/{id}/receive", post(receive))
        .route("/orders/{id}/return", post(request_return))
        .route("/return-media/{name}", get(return_media))
        .route("/orders/{id}/reviews", post(review))
}
const LINE: &str = "SELECT v.id,v.product_id,p.shop_id,p.name AS product_name,s.name AS shop,v.name AS variant,v.subvariant,v.price,v.stock,v.grams,(v.enabled AND p.status='active' AND NOT s.suspended) AS available FROM variants v JOIN products p ON p.id=v.product_id JOIN shops s ON s.id=p.shop_id";
#[derive(Serialize, sqlx::FromRow)]
struct Line {
    id: i64,
    product_id: i64,
    shop_id: i64,
    product_name: String,
    shop: String,
    variant: String,
    subvariant: String,
    price: i64,
    stock: i32,
    grams: i32,
    available: bool,
}
#[derive(Deserialize)]
struct Quantity {
    quantity: i32,
    #[serde(default)]
    imported: bool,
}
const SET_CART: &str = "INSERT INTO carts(user_id,variant_id,quantity) VALUES (?,?,?)
    ON DUPLICATE KEY UPDATE quantity=VALUES(quantity)";
async fn cart(State(app): State<App>, user: User) -> Result {
    Ok(Json(json!(decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('id',v.id,'product_id',p.id,'product_name',p.name,'image',p.image,'shop',s.name,'variant',v.name,'subvariant',v.subvariant,'price',v.price,'quantity',c.quantity,'created_at',c.created_at,'reason',CASE WHEN NOT v.enabled OR p.status<>'active' OR s.suspended THEN 'Unavailable' WHEN v.stock=0 THEN 'Out of stock' WHEN v.stock<c.quantity THEN 'Insufficient stock' ELSE NULL END) FROM carts c JOIN variants v ON v.id=c.variant_id JOIN products p ON p.id=v.product_id JOIN shops s ON s.id=p.shop_id WHERE c.user_id=? ORDER BY c.created_at DESC")
        .bind(user.id).fetch_all(&app.db).await?)?)))
}
async fn availability(State(app): State<App>, Json(items): Json<Vec<Pick>>) -> Result {
    if items.len() > 100 {
        return Err(Error::bad("Select up to 100 items"));
    }
    let ids = json!(items.iter().map(|p| p.id).collect::<Vec<_>>()).to_string();
    let query = format!("{LINE} WHERE JSON_CONTAINS(?,JSON_ARRAY(v.id))");
    let lines: Vec<Line> = sqlx::query_as(sqlx::AssertSqlSafe(query))
        .bind(ids)
        .fetch_all(&app.db)
        .await?;
    Ok(Json(json!(items.into_iter().map(|pick| {
        let line = lines.iter().find(|line| line.id == pick.id);
        let reason = match line {
            Some(line) if line.available && line.stock == 0 => Some("Out of stock"),
            Some(line) if line.available && line.stock < pick.quantity => Some("Insufficient stock"),
            Some(line) if line.available => None,
            _ => Some("Unavailable"),
        };
        json!({"id":pick.id, "reason":reason, "price":line.filter(|line| line.available).map(|line| line.price)})
    }).collect::<Vec<_>>())))
}
async fn set_cart(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Quantity>,
) -> Result {
    if input.quantity == 0 {
        sqlx::query("DELETE FROM carts WHERE user_id=? AND variant_id=?")
            .bind(user.id)
            .bind(id)
            .execute(&app.db)
            .await?;
    } else {
        let line: Line = sqlx::query_as(sqlx::AssertSqlSafe(format!("{LINE} WHERE v.id=?")))
            .bind(id)
            .fetch_one(&app.db)
            .await?;
        validate(&line, input.quantity)?;
        if app.recommendations.enabled() && !user.is_guest() && !input.imported {
            let mut retries = 2;
            loop {
                match set_cart_and_record(&app.db, user.id, &line, input.quantity).await {
                    Ok(()) => break,
                    Err(error)
                        if retries > 0
                            && error
                                .as_database_error()
                                .is_some_and(|error| error.code().as_deref() == Some("40001")) =>
                    {
                        // Concurrent first updates can both observe a missing cart row.
                        retries -= 1;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
        } else {
            sqlx::query(SET_CART)
                .bind(user.id)
                .bind(id)
                .bind(input.quantity)
                .execute(&app.db)
                .await?;
        }
    }
    Ok(ok())
}

async fn set_cart_and_record(
    db: &MySqlPool,
    user: i64,
    line: &Line,
    quantity: i32,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    let prior: Option<i32> = sqlx::query_scalar(
        "SELECT quantity FROM carts WHERE user_id=? AND variant_id=? FOR UPDATE",
    )
    .bind(user)
    .bind(line.id)
    .fetch_optional(&mut *tx)
    .await?;
    sqlx::query(SET_CART)
        .bind(user)
        .bind(line.id)
        .bind(quantity)
        .execute(&mut *tx)
        .await?;
    if quantity > prior.unwrap_or(0) {
        crate::recommendations::record_cart(&mut tx, user, line.product_id).await?;
    }
    tx.commit().await
}
async fn addresses(State(app): State<App>, user: User) -> Result {
    Ok(Json(json!(decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('id',id,'label',label,'detail',detail) FROM addresses WHERE user_id=? ORDER BY id DESC").bind(user.id).fetch_all(&app.db).await?)?)))
}
#[derive(Serialize, Deserialize)]
struct Address {
    label: String,
    detail: String,
    key: String,
}
async fn add_address(State(app): State<App>, user: User, Json(input): Json<Address>) -> Result {
    let mut tx = app.db.begin().await?;
    let id = if let Some(id) = claim(&mut tx, user.id, "address", &input.key, &input).await? {
        id
    } else {
        let id = inserted(
            sqlx::query("INSERT INTO addresses(user_id,label,detail) VALUES (?,?,?)")
                .bind(user.id)
                .bind(text(&input.label, 100)?)
                .bind(text(&input.detail, 1000)?)
                .execute(&mut *tx)
                .await?,
        )?;
        finish(&mut tx, user.id, "address", &input.key, id).await?;
        id
    };
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Serialize, Deserialize)]
struct Pick {
    id: i64,
    quantity: i32,
}
#[derive(Serialize, Deserialize)]
struct Checkout {
    items: Vec<Pick>,
    address: i64,
    key: String,
    #[serde(default)]
    from_cart: bool,
    expected_total: Option<i64>,
}
#[derive(Serialize)]
struct Chosen {
    #[serde(flatten)]
    line: Line,
    quantity: i32,
}
#[derive(Serialize, Default)]
struct Group {
    shop: String,
    items: Vec<Chosen>,
    subtotal: i64,
    shipping: i64,
    tax: i64,
    grams: i64,
}
#[derive(Serialize)]
struct Quote {
    address: String,
    groups: BTreeMap<i64, Group>,
    subtotal: i64,
    shipping: i64,
    tax: i64,
    total: i64,
}
fn validate(line: &Line, qty: i32) -> Result<()> {
    if !line.available || !(1..=999).contains(&qty) || qty > line.stock {
        return Err(Error::conflict(
            "An item is unavailable or has insufficient stock",
        ));
    }
    Ok(())
}
async fn calculate(
    app: &App,
    tx: &mut Transaction<'_, MySql>,
    user: i64,
    input: &Checkout,
    lock: bool,
) -> Result<Quote> {
    if input.items.is_empty() || input.items.len() > 100 {
        return Err(Error::bad("Select 1 to 100 items"));
    }
    let address: String = sqlx::query_scalar(
        "SELECT CONCAT(label,': ',detail) FROM addresses WHERE user_id=? AND id=?",
    )
    .bind(user)
    .bind(input.address)
    .fetch_one(&mut **tx)
    .await?;
    let mut picks: Vec<_> = input.items.iter().collect();
    picks.sort_by_key(|p| p.id);
    if picks.windows(2).any(|pair| pair[0].id == pair[1].id) {
        return Err(Error::bad("Each variant can appear once"));
    }
    let mut groups = BTreeMap::<i64, Group>::new();
    for pick in picks {
        let query = format!(
            "{LINE} WHERE v.id=?{}",
            if lock { " FOR UPDATE" } else { "" }
        );
        let line: Line = sqlx::query_as(sqlx::AssertSqlSafe(query))
            .bind(pick.id)
            .fetch_one(&mut **tx)
            .await?;
        validate(&line, pick.quantity)?;
        if input.from_cart {
            let qty: Option<i32> = sqlx::query_scalar(if lock {
                "SELECT quantity FROM carts WHERE user_id=? AND variant_id=? FOR UPDATE"
            } else {
                "SELECT quantity FROM carts WHERE user_id=? AND variant_id=?"
            })
            .bind(user)
            .bind(pick.id)
            .fetch_optional(&mut **tx)
            .await?;
            if qty.unwrap_or(0) < pick.quantity {
                return Err(Error::conflict(
                    "Your cart has changed. Refresh it and try again.",
                ));
            }
        }
        let group = groups.entry(line.shop_id).or_default();
        group.shop.clone_from(&line.shop);
        group.subtotal += line.price * i64::from(pick.quantity);
        group.grams += i64::from(line.grams) * i64::from(pick.quantity);
        group.items.push(Chosen {
            line,
            quantity: pick.quantity,
        });
    }
    let (mut subtotal, mut shipping, mut tax) = (0, 0, 0);
    for g in groups.values_mut() {
        g.shipping = g.grams; // One rupiah per gram.
        g.tax = (g.subtotal * app.tax + 5000) / 10000;
        subtotal += g.subtotal;
        shipping += g.shipping;
        tax += g.tax;
    }
    Ok(Quote {
        address,
        groups,
        subtotal,
        shipping,
        tax,
        total: subtotal + shipping + tax,
    })
}
async fn quote(State(app): State<App>, user: User, Json(input): Json<Checkout>) -> Result {
    let mut tx = app.db.begin().await?;
    let quote = calculate(&app, &mut tx, user.id, &input, false).await?;
    tx.commit().await?;
    Ok(Json(json!(quote)))
}
async fn checkout(State(app): State<App>, user: User, Json(input): Json<Checkout>) -> Result {
    let mut tx = app.db.begin().await?;
    if let Some(id) = claim(&mut tx, user.id, "checkout", &input.key, &input).await? {
        return Ok(Json(json!({"id":id})));
    }
    let q = calculate(&app, &mut tx, user.id, &input, true).await?;
    if input.expected_total.is_some_and(|total| total != q.total) {
        return Err(Error::conflict(
            "The checkout total changed. Review the updated prices.",
        ));
    }
    let id=inserted(sqlx::query("INSERT INTO checkouts(checkout_no,user_id,address,subtotal,tax,shipping,total) VALUES (?,?,?,?,?,?,?)").bind(uuid::Uuid::new_v4().to_string()).bind(user.id).bind(q.address).bind(q.subtotal).bind(q.tax).bind(q.shipping).bind(q.total).execute(&mut *tx).await?)?;
    for (shop, g) in q.groups {
        let order=inserted(sqlx::query("INSERT INTO orders(checkout_id,shop_id,shipping,subtotal,tax,actor_id) VALUES (?,?,?,?,?,?)").bind(id).bind(shop).bind(g.shipping).bind(g.subtotal).bind(g.tax).bind(user.id).execute(&mut *tx).await?)?;
        for item in g.items {
            let line = item.line;
            let variant = if line.subvariant.is_empty() {
                line.variant
            } else {
                format!("{} / {}", line.variant, line.subvariant)
            };
            sqlx::query("INSERT INTO order_lines(order_id,variant_id,product_id,product_name,variant_name,price,quantity,grams) VALUES (?,?,?,?,?,?,?,?)").bind(order).bind(line.id).bind(line.product_id).bind(line.product_name).bind(variant).bind(line.price).bind(item.quantity).bind(line.grams).execute(&mut *tx).await?;
            sqlx::query("UPDATE variants SET stock=stock-?,actor_id=? WHERE id=?")
                .bind(item.quantity)
                .bind(user.id)
                .bind(line.id)
                .execute(&mut *tx)
                .await?;
            if input.from_cart {
                let deleted = sqlx::query(
                    "DELETE FROM carts WHERE user_id=? AND variant_id=? AND quantity=?",
                )
                .bind(user.id)
                .bind(line.id)
                .bind(item.quantity)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if deleted == 0 {
                    sqlx::query(
                        "UPDATE carts SET quantity=quantity-? WHERE user_id=? AND variant_id=?",
                    )
                    .bind(item.quantity)
                    .bind(user.id)
                    .bind(line.id)
                    .execute(&mut *tx)
                    .await?;
                }
            }
        }
    }
    finish(&mut tx, user.id, "checkout", &input.key, id).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
const SUMMARY: &str = "SELECT JSON_OBJECT('id',c.id,'checkout_no',c.checkout_no,'created_at',c.created_at,'address',c.address,'subtotal',c.subtotal,'tax',c.tax,'shipping',c.shipping,'total',c.total,'payment',c.payment,'status',CASE WHEN c.payment='cancelled' THEN 'Cancelled' WHEN c.payment='waiting' THEN 'Waiting for payment' WHEN EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status='waiting_shop') THEN IF(EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status<>'waiting_shop'),'Progress partially','Waiting shop response') WHEN NOT EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status NOT IN ('rejected','completed','return_review','return_accepted','return_rejected','return_completed')) THEN IF(EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status<>'rejected'),'Completed','Progress') WHEN EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status='delivery') THEN IF(EXISTS(SELECT 1 FROM orders o WHERE o.checkout_id=c.id AND o.status='packing'),'Partially on delivery','On delivery') ELSE 'Progress' END) FROM checkouts c";

pub(super) fn before_date(
    query: &mut sqlx::QueryBuilder<MySql>,
    cursor: Option<(chrono::NaiveDateTime, i64)>,
) {
    if let Some((created_at, id)) = cursor {
        query
            .push(" AND (created_at<")
            .push_bind(created_at)
            .push(" OR (created_at=")
            .push_bind(created_at)
            .push(" AND id<")
            .push_bind(id)
            .push("))");
    }
}

async fn checkouts(State(app): State<App>, user: User, Query(f): Query<Filter>) -> Result {
    let before = if let Some(id) = f.cursor {
        let position =
            sqlx::query_as("SELECT created_at,id FROM checkouts WHERE id=? AND user_id=?")
                .bind(id)
                .bind(user.id)
                .fetch_optional(&app.db)
                .await?;
        let Some(position) = position else {
            return Ok(catalog::page(Vec::new()));
        };
        Some(position)
    } else {
        None
    };
    let mut query = sqlx::QueryBuilder::new(SUMMARY);
    query
        .push(" FORCE INDEX (checkout_user_date) WHERE c.user_id=")
        .push_bind(user.id);
    before_date(&mut query, before);
    if let Some(search) = f.q.as_deref().filter(|q| !q.is_empty()) {
        let pattern = format!("%{search}%");
        query.push(" AND (CONCAT_WS(' ',c.checkout_no,c.address,c.total,c.created_at,c.payment) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR EXISTS(SELECT 1 FROM orders o JOIN shops s ON s.id=o.shop_id
                 JOIN order_lines l ON l.order_id=o.id WHERE o.checkout_id=c.id
                   AND CONCAT_WS(' ',s.name,o.status,o.receipt,l.product_name,l.variant_name) LIKE ")
            .push_bind(pattern)
            .push("))");
    }
    query.push(" ORDER BY c.created_at DESC,c.id DESC LIMIT 21");
    let values = query.build_query_scalar().fetch_all(&app.db).await?;
    Ok(catalog::page(decode_rows(values)?))
}
pub(super) const ORDER_SELECT: &str = "SELECT JSON_OBJECT('id',o.id,'checkout_id',o.checkout_id,'shop',s.name,'subtotal',o.subtotal,'shipping',o.shipping,'tax',o.tax,'status',o.status,'receipt',o.receipt,'courier',co.name,'courier_id',co.id,'courier_url',co.url,'created_at',o.created_at,'completed_at',o.completed_at,'returnable',IF(o.status='completed' AND o.completed_at>=UTC_TIMESTAMP()-INTERVAL 14 DAY,1,0),'actor',u.name)";
pub(super) const ORDER_JOINS: &str = " JOIN shops s ON s.id=o.shop_id JOIN users u ON u.id=o.actor_id LEFT JOIN couriers co ON co.id=o.courier_id";
pub async fn populate(app: &App, mut order: Value) -> Result<Value> {
    let id = order["id"]
        .as_i64()
        .ok_or_else(|| Error::internal("Missing order ID"))?;
    order["items"]=json!(decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('product_id',l.product_id,'product_name',l.product_name,'variant_name',l.variant_name,'quantity',l.quantity,'price',l.price,'reviewed',EXISTS(SELECT 1 FROM reviews r WHERE r.order_id=l.order_id AND r.product_id=l.product_id)) FROM order_lines l WHERE l.order_id=? ORDER BY l.product_name,l.variant_id").bind(id).fetch_all(&app.db).await?)?);
    order["tracking"]=json!(decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('note',note,'created_at',created_at) FROM tracking WHERE order_id=? ORDER BY id DESC").bind(id).fetch_all(&app.db).await?)?);
    let raw:Option<String>=sqlx::query_scalar("SELECT JSON_OBJECT('description',description,'media',JSON_EXTRACT(media,'$')) FROM returns WHERE order_id=?").bind(id).fetch_optional(&app.db).await?;
    if let Some(raw) = raw {
        order["return"] = serde_json::from_str(&raw).map_err(Error::internal)?;
    }
    Ok(order)
}
async fn detail(State(app): State<App>, user: User, Path(id): Path<i64>) -> Result {
    let raw: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{SUMMARY} WHERE c.id=? AND (c.user_id=? OR ?)"
    )))
    .bind(id)
    .bind(user.id)
    .bind(user.admin().is_ok())
    .fetch_one(&app.db)
    .await?;
    let mut checkout: Value = serde_json::from_str(&raw).map_err(Error::internal)?;
    let mut orders = vec![];
    for order in decode_rows(
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "{ORDER_SELECT} FROM orders o{ORDER_JOINS} WHERE o.checkout_id=? ORDER BY o.id"
        )))
        .bind(id)
        .fetch_all(&app.db)
        .await?,
    )? {
        orders.push(populate(&app, order).await?);
    }
    checkout["orders"] = json!(orders);
    checkout["payment_instructions"] = json!(setting(
        "MANUAL_PAYMENT_INSTRUCTIONS",
        "Contact the marketplace administrator for manual payment instructions."
    ));
    Ok(Json(checkout))
}
async fn cancel(State(app): State<App>, user: User, Path(id): Path<i64>) -> Result {
    let mut tx = app.db.begin().await?;
    let paid: String =
        sqlx::query_scalar("SELECT payment FROM checkouts WHERE id=? AND user_id=? FOR UPDATE")
            .bind(id)
            .bind(user.id)
            .fetch_one(&mut *tx)
            .await?;
    if paid == "cancelled" {
        return Ok(ok());
    }
    if paid != "waiting" {
        return Err(Error::conflict("Only unpaid checkouts can be cancelled"));
    }
    let orders: Vec<i64> = sqlx::query_scalar("SELECT id FROM orders WHERE checkout_id=?")
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
    for order in orders {
        restore(&mut tx, order, user.id).await?;
    }
    sqlx::query("UPDATE orders SET status='cancelled',actor_id=? WHERE checkout_id=?")
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE checkouts SET payment='cancelled' WHERE id=?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ok())
}
pub async fn restore(tx: &mut Transaction<'_, MySql>, order: i64, actor: i64) -> Result<()> {
    sqlx::query("UPDATE variants v JOIN order_lines l ON l.variant_id=v.id SET v.stock=v.stock+l.quantity,v.actor_id=? WHERE l.order_id=?").bind(actor).bind(order).execute(&mut **tx).await?;
    Ok(())
}
async fn pay(State(app): State<App>, user: User, Path(id): Path<i64>) -> Result {
    user.admin()?;
    let mut tx = app.db.begin().await?;
    let (status, buyer): (String, i64) =
        sqlx::query_as("SELECT payment,user_id FROM checkouts WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if status == "paid" {
        return Ok(ok());
    }
    if status != "waiting" {
        return Err(Error::conflict("Checkout cannot be marked paid"));
    }
    sqlx::query("UPDATE checkouts SET payment='paid',paid_by=? WHERE id=?")
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE orders SET status='waiting_shop',actor_id=? WHERE checkout_id=?")
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let owners:Vec<i64>=sqlx::query_scalar("SELECT DISTINCT s.owner_id FROM orders o JOIN shops s ON s.id=o.shop_id WHERE o.checkout_id=?").bind(id).fetch_all(&mut *tx).await?;
    for owner in owners {
        notice(&mut tx, owner, "A paid order is ready for your response.").await?;
    }
    notice(&mut tx, buyer, "Your manual payment was confirmed.").await?;
    tx.commit().await?;
    Ok(ok())
}
async fn receive(State(app): State<App>, user: User, Path(id): Path<i64>) -> Result {
    let count=sqlx::query("UPDATE orders o JOIN checkouts c ON c.id=o.checkout_id SET o.status='completed',o.completed_at=UTC_TIMESTAMP(),o.actor_id=? WHERE o.id=? AND c.user_id=? AND o.status='delivery'").bind(user.id).bind(id).bind(user.id).execute(&app.db).await?.rows_affected();
    if count == 0 {
        return Err(Error::conflict(
            "Only dispatched orders can be confirmed received",
        ));
    }
    Ok(ok())
}
#[derive(Deserialize)]
struct Return {
    description: String,
    #[serde(default)]
    media: Vec<String>,
}
fn validate_media(media: &[String]) -> Result<()> {
    if media.len() > 5 {
        return Err(Error::bad("Attach up to five files"));
    }
    for file in media {
        catalog::media(file, false)?;
    }
    Ok(())
}
#[derive(Deserialize)]
struct ReturnMediaOptions {
    size: Option<ImageSize>,
}
async fn return_media(
    State(app): State<App>,
    user: User,
    Path(name): Path<String>,
    Query(options): Query<ReturnMediaOptions>,
) -> Result<Response> {
    let (_, mime) = catalog::return_media_name(&name).ok_or_else(Error::missing)?;
    let path = format!("{}{name}", catalog::RETURN_MEDIA_PREFIX);
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM returns r JOIN orders o ON o.id=r.order_id JOIN checkouts c ON c.id=o.checkout_id JOIN shops s ON s.id=o.shop_id WHERE JSON_CONTAINS(r.media, JSON_QUOTE(?)) AND (c.user_id=? OR ? OR (NOT s.suspended AND EXISTS(SELECT 1 FROM members m WHERE m.shop_id=o.shop_id AND m.user_id=? AND m.active))))")
        .bind(path).bind(user.id).bind(user.admin().is_ok()).bind(user.id).fetch_one(&app.db).await?;
    if !allowed {
        return Err(Error::missing());
    }
    let bytes = uploads::read_private(&name, options.size)
        .await
        .map_err(|_| Error::missing())?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, mime),
            (axum::http::header::CACHE_CONTROL, "private, no-store"),
            (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
async fn request_return(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Return>,
) -> Result {
    if input.media.is_empty() || input.media.len() > 5 {
        return Err(Error::bad("Attach one to five photos or videos"));
    }
    for file in &input.media {
        let name = file
            .strip_prefix(catalog::RETURN_MEDIA_PREFIX)
            .ok_or_else(|| Error::bad("Upload return evidence first"))?;
        if !catalog::return_media_name(name).is_some_and(|(owner, _)| owner == user.id)
            || !tokio::fs::try_exists(std::path::Path::new(uploads::PRIVATE_DIR).join(name))
                .await
                .map_err(Error::internal)?
        {
            return Err(Error::bad("Upload return evidence first"));
        }
    }
    let mut tx = app.db.begin().await?;
    let changed=sqlx::query("UPDATE orders o JOIN checkouts c ON c.id=o.checkout_id SET o.status='return_review',o.actor_id=? WHERE o.id=? AND c.user_id=? AND o.status='completed' AND o.completed_at>=UTC_TIMESTAMP()-INTERVAL 14 DAY").bind(user.id).bind(id).bind(user.id).execute(&mut *tx).await?.rows_affected();
    if changed == 0 {
        return Err(Error::conflict(
            "This order is outside the return window or already has a return",
        ));
    }
    sqlx::query("INSERT INTO returns(order_id,description,media) VALUES (?,?,?)")
        .bind(id)
        .bind(text(&input.description, 4000)?)
        .bind(json!(input.media).to_string())
        .execute(&mut *tx)
        .await?;
    let owner: i64 = sqlx::query_scalar(
        "SELECT s.owner_id FROM orders o JOIN shops s ON s.id=o.shop_id WHERE o.id=?",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    notice(&mut tx, owner, "A customer requested a return.").await?;
    tx.commit().await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct Review {
    product_id: i64,
    rating: i32,
    body: String,
    #[serde(default)]
    media: Vec<String>,
}
async fn review(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Review>,
) -> Result {
    validate_media(&input.media)?;
    if !(1..=5).contains(&input.rating) {
        return Err(Error::bad("Select a rating between 1 and 5"));
    }
    let changed=sqlx::query("INSERT INTO reviews(order_id,product_id,user_id,rating,body,media,actor_id) SELECT o.id,?,?,?, ?,?,? FROM orders o JOIN checkouts c ON c.id=o.checkout_id WHERE o.id=? AND c.user_id=? AND o.completed_at IS NOT NULL AND EXISTS(SELECT 1 FROM order_lines l WHERE l.order_id=o.id AND l.product_id=?)")
        .bind(input.product_id).bind(user.id).bind(input.rating).bind(text(&input.body,4000)?).bind(json!(input.media).to_string()).bind(user.id).bind(id).bind(user.id).bind(input.product_id).execute(&app.db).await?.rows_affected();
    if changed == 0 {
        return Err(Error::forbidden());
    }
    Ok(ok())
}
