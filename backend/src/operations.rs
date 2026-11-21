use super::*;
use crate::{
    auth::{self, User},
    catalog::{self, Filter, decode_rows},
    commerce,
};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlDatabaseError;

mod history;

pub fn routes() -> Router<App> {
    Router::new()
        .merge(history::routes())
        .route("/merchant/{shop}/dashboard", get(shop_dash))
        .route(
            "/merchant/{shop}/products",
            get(products).post(create_product),
        )
        .route(
            "/merchant/{shop}/products/{id}",
            get(product).put(edit_product),
        )
        .route("/merchant/{shop}/products/{id}/variants", post(add_variant))
        .route(
            "/merchant/{shop}/products/{id}/variants/{variant}",
            put(update_variant),
        )
        .route("/merchant/{shop}/orders", get(orders))
        .route(
            "/merchant/{shop}/orders/{id}",
            get(order).post(update_order),
        )
        .route("/merchant/{shop}/reviews/{id}", put(reply))
        .route("/merchant/{shop}/team", get(team).post(add_team))
        .route(
            "/merchant/{shop}/team/{employee}",
            get(team_member).put(update_employee),
        )
        .route("/admin/dashboard", get(admin_dash))
        .route("/admin/{section}", get(admin_list))
        .route("/admin/products/{id}", put(moderate_product))
        .route("/admin/taxonomy/{id}", put(moderate_taxonomy))
        .route("/admin/reviews/{id}", put(delete_review))
        .route("/admin/users/{id}", put(update_user))
        .route("/admin/shops/{id}", put(suspend_shop))
        .route("/admin/accounts", post(add_admin))
        .route("/admin/couriers", get(admin_couriers).post(save_courier))
        .route("/admin/taxonomy", get(admin_taxonomy).post(save_taxonomy))
}
async fn member(app: &App, user: &User, shop: i64, owner: bool) -> Result<()> {
    let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM shops s JOIN members m ON m.shop_id=s.id WHERE s.id=? AND m.user_id=? AND m.active AND NOT s.suspended AND (NOT ? OR s.owner_id=m.user_id))").bind(shop).bind(user.id).bind(owner).fetch_one(&app.db).await?;
    if !allowed {
        return Err(Error::forbidden());
    }
    Ok(())
}
const PRODUCT_SELECT: &str = "SELECT JSON_OBJECT('id',p.id,'name',p.name,'image',p.image,'description',p.description,'status',p.status,'note',p.note,'brand',b.name,'category',c.name,'shop',s.name,'created_at',p.created_at,'actor',u.name,'lowest_price',(SELECT MIN(price) FROM variants WHERE product_id=p.id AND enabled),'stock',COALESCE((SELECT SUM(stock) FROM variants WHERE product_id=p.id AND enabled),0),'recent_sold',COALESCE((SELECT SUM(l.quantity) FROM order_lines l JOIN orders o ON o.id=l.order_id WHERE l.product_id=p.id AND o.completed_at>=UTC_TIMESTAMP()-INTERVAL 28 DAY),0),'sold',COALESCE((SELECT SUM(l.quantity) FROM order_lines l JOIN orders o ON o.id=l.order_id WHERE l.product_id=p.id AND o.completed_at IS NOT NULL),0))";
const PRODUCT_JOINS: &str = " JOIN shops s ON s.id=p.shop_id JOIN users u ON u.id=p.actor_id LEFT JOIN taxonomy b ON b.id=p.brand_id LEFT JOIN taxonomy c ON c.id=p.category_id";

fn product_page_query(index: &'static str) -> sqlx::QueryBuilder<MySql> {
    let mut query = sqlx::QueryBuilder::new(PRODUCT_SELECT);
    query
        .push(" FROM (SELECT p.id FROM products p FORCE INDEX (")
        .push(index)
        .push(")");
    query
}

async fn product_page(
    db: &MySqlPool,
    mut query: sqlx::QueryBuilder<MySql>,
    sort: &'static str,
    limit: u32,
) -> Result<Vec<String>> {
    // Perform stock and sales lookups only for the listings on this page.
    query
        .push(" ORDER BY p.id ")
        .push(sort)
        .push(" LIMIT ")
        .push_bind(limit)
        .push(") page STRAIGHT_JOIN products p FORCE INDEX (PRIMARY) ON p.id=page.id")
        .push(PRODUCT_JOINS)
        .push(" ORDER BY p.id ")
        .push(sort);
    Ok(query.build_query_scalar().fetch_all(db).await?)
}

async fn products(
    State(app): State<App>,
    user: User,
    Path(shop): Path<i64>,
    Query(f): Query<Filter>,
) -> Result {
    member(&app, &user, shop, false).await?;
    let status = f.status.as_deref().filter(|status| !status.is_empty());
    let index = if status.is_some() {
        "product_shop_status_id"
    } else {
        "shop_id"
    };
    let mut query = product_page_query(index);
    query
        .push(" WHERE p.shop_id=")
        .push_bind(shop)
        .push(" AND p.id<")
        .push_bind(f.cursor.unwrap_or(i64::MAX));
    if let Some(status) = status {
        query.push(" AND p.status=").push_bind(status);
    }
    if let Some(search) = f.q.as_deref().filter(|q| !q.is_empty()) {
        query
            .push(" AND p.name LIKE ")
            .push_bind(format!("%{search}%"));
    }
    let rows = product_page(&app.db, query, "DESC", 21).await?;
    Ok(catalog::page(decode_rows(rows)?))
}
fn period(value: Option<&str>) -> (&'static str, i32) {
    match value {
        Some("week") => ("%x-W%v", 84),
        Some("month") => ("%Y-%m", 365),
        Some("year") => ("%Y", 3650),
        _ => ("%Y-%m-%d", 30),
    }
}
async fn series(app: &App, format: &str, days: i32, shop: i64, product: i64) -> Result<Vec<Value>> {
    let mut query =
        sqlx::QueryBuilder::new("SELECT JSON_OBJECT('label',DATE_FORMAT(o.completed_at,");
    query.push_bind(format);
    if product > 0 {
        query.push("), 'value',SUM(l.quantity)) FROM orders o JOIN order_lines l ON l.order_id=o.id WHERE l.product_id=")
            .push_bind(product).push(" AND o.shop_id=").push_bind(shop).push(" AND ");
    } else {
        query.push("), 'value',SUM(o.subtotal)) FROM orders o WHERE ");
        if shop != 0 {
            query.push("o.shop_id=").push_bind(shop).push(" AND ");
        }
    }
    query
        .push("o.completed_at>=UTC_TIMESTAMP()-INTERVAL ")
        .push_bind(days)
        .push(" DAY GROUP BY DATE_FORMAT(o.completed_at,")
        .push_bind(format)
        .push(") ORDER BY DATE_FORMAT(o.completed_at,")
        .push_bind(format)
        .push(")");
    decode_rows(query.build_query_scalar().fetch_all(&app.db).await?)
}
async fn product(
    State(app): State<App>,
    user: User,
    Path((shop, id)): Path<(i64, i64)>,
    Query(f): Query<Filter>,
) -> Result {
    member(&app, &user, shop, false).await?;
    let raw: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{PRODUCT_SELECT} FROM products p{PRODUCT_JOINS} WHERE p.id=? AND p.shop_id=?"
    )))
    .bind(id)
    .bind(shop)
    .fetch_one(&app.db)
    .await?;
    let mut item: Value = serde_json::from_str(&raw).map_err(Error::internal)?;
    item["variants"] = json!(catalog::variants(&app, id, true).await?);
    item["reviews"] = json!(catalog::reviews(&app, id).await?);
    let (format, days) = period(f.status.as_deref());
    item["series"] = json!(series(&app, format, days, shop, id).await?);
    let sort = if f.sort.as_deref() == Some("oldest") {
        "ASC"
    } else {
        "DESC"
    };
    let sql = format!(
        "{} FROM orders o{} WHERE o.shop_id=? AND EXISTS(SELECT 1 FROM order_lines l WHERE l.order_id=o.id AND l.product_id=?) ORDER BY o.created_at {sort},o.id {sort} LIMIT 100",
        commerce::ORDER_SELECT,
        commerce::ORDER_JOINS,
    );
    item["orders"] = json!(decode_rows(
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .bind(shop)
            .bind(id)
            .fetch_all(&app.db)
            .await?
    )?);
    item["counts"]=json!(decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('status',o.status,'value',COUNT(*)) FROM orders o WHERE o.shop_id=? AND EXISTS(SELECT 1 FROM order_lines l WHERE l.order_id=o.id AND l.product_id=?) GROUP BY o.status").bind(shop).bind(id).fetch_all(&app.db).await?)?);
    Ok(Json(item))
}
#[derive(Serialize, Deserialize)]
struct Variant {
    name: String,
    #[serde(default)]
    subvariant: String,
    price: i64,
    stock: i32,
    grams: i32,
    #[serde(default = "enabled")]
    enabled: bool,
    key: Option<String>,
}
fn enabled() -> bool {
    true
}
fn validate_variant(v: &Variant) -> Result<()> {
    text(&v.name, 150)?;
    if v.subvariant.chars().count() > 150
        || !(1..=1_000_000_000).contains(&v.price)
        || !(0..=100_000_000).contains(&v.stock)
        || !(1..=1_000_000).contains(&v.grams)
    {
        return Err(Error::bad("Check the variant price, stock and weight"));
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Product {
    name: String,
    description: String,
    #[serde(default)]
    image: String,
    brand: Option<String>,
    category: Option<String>,
    variant: Option<Variant>,
    key: Option<String>,
}
async fn taxonomy(
    tx: &mut Transaction<'_, MySql>,
    kind: &str,
    name: Option<&str>,
) -> Result<Option<i64>> {
    let Some(name) = name.filter(|n| !n.trim().is_empty()) else {
        return Ok(None);
    };
    let name = text(name, 120)?;
    sqlx::query("INSERT INTO taxonomy(kind,name) VALUES (?,?) ON DUPLICATE KEY UPDATE name=name")
        .bind(kind)
        .bind(name)
        .execute(&mut **tx)
        .await?;
    Ok(Some(
        sqlx::query_scalar("SELECT id FROM taxonomy WHERE kind=? AND name=?")
            .bind(kind)
            .bind(name)
            .fetch_one(&mut **tx)
            .await?,
    ))
}
async fn insert_variant(
    tx: &mut Transaction<'_, MySql>,
    id: i64,
    user: i64,
    v: &Variant,
) -> Result<i64> {
    validate_variant(v)?;
    inserted(sqlx::query("INSERT INTO variants(product_id,name,subvariant,price,stock,grams,enabled,actor_id) VALUES (?,?,?,?,?,?,?,?)").bind(id).bind(v.name.trim()).bind(v.subvariant.trim()).bind(v.price).bind(v.stock).bind(v.grams).bind(v.enabled).bind(user).execute(&mut **tx).await?)
}
fn check_product(input: &Product) -> Result<()> {
    text(&input.name, 180)?;
    text(&input.description, 12000)?;
    if !input.image.is_empty() && !input.image.starts_with("/catalog/") {
        catalog::media(&input.image, true)?;
    }
    Ok(())
}
fn key(value: Option<&str>) -> Result<&str> {
    value.ok_or_else(|| Error::bad("Request key required"))
}
async fn create_product(
    State(app): State<App>,
    user: User,
    Path(shop): Path<i64>,
    Json(input): Json<Product>,
) -> Result {
    member(&app, &user, shop, false).await?;
    check_product(&input)?;
    if input.image.starts_with("/catalog/") {
        return Err(Error::bad("Upload the product image first"));
    }
    let request = key(input.key.as_deref())?;
    let scope = format!("product/{shop}");
    let mut tx = app.db.begin().await?;
    if let Some(id) = claim(&mut tx, user.id, &scope, request, &input).await? {
        return Ok(Json(json!({"id":id})));
    }
    let brand = taxonomy(&mut tx, "brand", input.brand.as_deref()).await?;
    let category = taxonomy(&mut tx, "category", input.category.as_deref()).await?;
    let id=inserted(sqlx::query("INSERT INTO products(shop_id,name,description,image,brand_id,category_id,actor_id) VALUES (?,?,?,?,?,?,?)").bind(shop).bind(input.name.trim()).bind(input.description.trim()).bind(&input.image).bind(brand).bind(category).bind(user.id).execute(&mut *tx).await?)?;
    insert_variant(
        &mut tx,
        id,
        user.id,
        input
            .variant
            .as_ref()
            .ok_or_else(|| Error::bad("Add the first variant"))?,
    )
    .await?;
    finish(&mut tx, user.id, &scope, request, id).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
async fn edit_product(
    State(app): State<App>,
    user: User,
    Path((shop, id)): Path<(i64, i64)>,
    Json(input): Json<Product>,
) -> Result {
    member(&app, &user, shop, false).await?;
    check_product(&input)?;
    let mut tx = app.db.begin().await?;
    let current: String =
        sqlx::query_scalar("SELECT image FROM products WHERE id=? AND shop_id=? FOR UPDATE")
            .bind(id)
            .bind(shop)
            .fetch_one(&mut *tx)
            .await?;
    if input.image.starts_with("/catalog/") && current != input.image {
        return Err(Error::bad("Upload the product image first"));
    }
    let brand = taxonomy(&mut tx, "brand", input.brand.as_deref()).await?;
    let category = taxonomy(&mut tx, "category", input.category.as_deref()).await?;
    sqlx::query("UPDATE products SET name=?,description=?,image=?,brand_id=?,category_id=?,status='pending',note='',actor_id=? WHERE id=?").bind(input.name.trim()).bind(input.description.trim()).bind(input.image).bind(brand).bind(category).bind(user.id).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(ok())
}
async fn add_variant(
    State(app): State<App>,
    user: User,
    Path((shop, id)): Path<(i64, i64)>,
    Json(input): Json<Variant>,
) -> Result {
    member(&app, &user, shop, false).await?;
    let request = key(input.key.as_deref())?;
    let scope = format!("variant/{id}");
    let mut tx = app.db.begin().await?;
    if let Some(id) = claim(&mut tx, user.id, &scope, request, &input).await? {
        return Ok(Json(json!({"id":id})));
    }
    let _: i64 = sqlx::query_scalar("SELECT id FROM products WHERE id=? AND shop_id=?")
        .bind(id)
        .bind(shop)
        .fetch_one(&mut *tx)
        .await?;
    let variant = insert_variant(&mut tx, id, user.id, &input).await?;
    finish(&mut tx, user.id, &scope, request, variant).await?;
    tx.commit().await?;
    Ok(Json(json!({"id":variant})))
}
async fn update_variant(
    State(app): State<App>,
    user: User,
    Path((shop, id, variant)): Path<(i64, i64, i64)>,
    Json(input): Json<Variant>,
) -> Result {
    member(&app, &user, shop, false).await?;
    validate_variant(&input)?;
    sqlx::query("UPDATE variants v JOIN products p ON p.id=v.product_id SET v.name=?,v.subvariant=?,v.price=?,v.stock=?,v.grams=?,v.enabled=?,v.actor_id=? WHERE v.id=? AND p.id=? AND p.shop_id=?")
        .bind(input.name.trim()).bind(input.subvariant.trim()).bind(input.price).bind(input.stock).bind(input.grams).bind(input.enabled).bind(user.id).bind(variant).bind(id).bind(shop).execute(&app.db).await?;
    Ok(ok())
}
enum OrderStatus<'a> {
    Any,
    Exact(&'a str),
    ExceptWaiting,
}

async fn order_page(
    db: &MySqlPool,
    shop: i64,
    before: Option<(time::PlainDateTime, i64)>,
    status: OrderStatus<'_>,
    limit: u32,
) -> Result<Vec<Value>> {
    let index = match status {
        OrderStatus::Exact(_) => "order_shop_status_date",
        _ => "order_shop_date",
    };
    let mut query = sqlx::QueryBuilder::new(commerce::ORDER_SELECT);
    query
        .push(" FROM (SELECT id FROM orders FORCE INDEX (")
        .push(index)
        .push(") WHERE shop_id=")
        .push_bind(shop);
    match status {
        OrderStatus::Exact(status) => {
            query.push(" AND status=").push_bind(status);
        }
        OrderStatus::ExceptWaiting => {
            query.push(" AND status<>'waiting_shop'");
        }
        OrderStatus::Any => {}
    }
    commerce::before_date(&mut query, before);
    query
        .push(" ORDER BY created_at DESC,id DESC LIMIT ")
        .push_bind(limit)
        .push(") page STRAIGHT_JOIN orders o FORCE INDEX (PRIMARY) ON o.id=page.id")
        .push(commerce::ORDER_JOINS)
        .push(" ORDER BY o.created_at DESC,o.id DESC");
    decode_rows(query.build_query_scalar().fetch_all(db).await?)
}

async fn orders_list(app: &App, shop: i64, f: Filter, priority: bool) -> Result<Vec<Value>> {
    let before = if let Some(id) = f.cursor {
        let position = sqlx::query_as("SELECT created_at,id FROM orders WHERE id=? AND shop_id=?")
            .bind(id)
            .bind(shop)
            .fetch_optional(&app.db)
            .await?;
        let Some(position) = position else {
            return Ok(Vec::new());
        };
        Some(position)
    } else {
        None
    };
    const LIMIT: u32 = 21;
    if let Some(status) = f.status.as_deref().filter(|status| !status.is_empty()) {
        return order_page(&app.db, shop, before, OrderStatus::Exact(status), LIMIT).await;
    }
    if !priority {
        return order_page(&app.db, shop, before, OrderStatus::Any, LIMIT).await;
    }

    // The dashboard puts waiting orders first. Read that page from its own index range.
    let mut orders = order_page(
        &app.db,
        shop,
        before,
        OrderStatus::Exact("waiting_shop"),
        LIMIT,
    )
    .await?;
    if orders.len() < LIMIT as usize {
        // An order can leave waiting_shop between the two reads.
        let remaining = LIMIT as usize - orders.len();
        let waiting_ids: Vec<_> = orders
            .iter()
            .filter_map(|order| order["id"].as_i64())
            .collect();
        let others = order_page(&app.db, shop, before, OrderStatus::ExceptWaiting, LIMIT).await?;
        orders.extend(
            others
                .into_iter()
                .filter(|order| {
                    order["id"]
                        .as_i64()
                        .is_some_and(|id| !waiting_ids.contains(&id))
                })
                .take(remaining),
        );
    }
    Ok(orders)
}
async fn orders(
    State(app): State<App>,
    user: User,
    Path(shop): Path<i64>,
    Query(f): Query<Filter>,
) -> Result {
    member(&app, &user, shop, false).await?;
    Ok(catalog::page(orders_list(&app, shop, f, false).await?))
}
async fn order(State(app): State<App>, user: User, Path((shop, id)): Path<(i64, i64)>) -> Result {
    member(&app, &user, shop, false).await?;
    let raw = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "{} FROM orders o{} WHERE o.shop_id=? AND o.id=?",
        commerce::ORDER_SELECT,
        commerce::ORDER_JOINS,
    )))
    .bind(shop)
    .bind(id)
    .fetch_one(&app.db)
    .await?;
    let mut values = decode_rows(vec![raw])?;
    let mut data = commerce::populate(&app, values.pop().ok_or_else(Error::missing)?).await?;
    data["address"] = json!(
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT IF(c.payment='paid',c.address,NULL) FROM checkouts c JOIN orders o ON o.checkout_id=c.id WHERE o.id=? AND o.shop_id=?"
        )
        .bind(id)
        .bind(shop)
        .fetch_one(&app.db)
        .await?
    );
    Ok(Json(data))
}
#[derive(Deserialize)]
struct OrderUpdate {
    status: String,
    courier: Option<i64>,
    receipt: Option<String>,
    note: Option<String>,
    key: Option<String>,
}
async fn update_order(
    State(app): State<App>,
    user: User,
    Path((shop, id)): Path<(i64, i64)>,
    Json(input): Json<OrderUpdate>,
) -> Result {
    member(&app, &user, shop, false).await?;
    let mut tx = app.db.begin().await?;
    let (current,courier,buyer):(String,Option<i64>,i64)=sqlx::query_as("SELECT o.status,o.courier_id,c.user_id FROM orders o JOIN checkouts c ON c.id=o.checkout_id WHERE o.id=? AND o.shop_id=? FOR UPDATE").bind(id).bind(shop).fetch_one(&mut *tx).await?;
    let next = input.status.as_str();
    if next == "tracking" && current == "delivery" && courier == Some(1) {
        let request = key(input.key.as_deref())?;
        let note = text(input.note.as_deref().unwrap_or(""), 1000)?;
        let scope = format!("tracking/{id}");
        if claim(&mut tx, user.id, &scope, request, &note)
            .await?
            .is_none()
        {
            sqlx::query("INSERT INTO tracking(order_id,note,actor_id) VALUES (?,?,?)")
                .bind(id)
                .bind(note)
                .bind(user.id)
                .execute(&mut *tx)
                .await?;
            finish(&mut tx, user.id, &scope, request, id).await?;
        }
    } else if current == next {
        return Ok(ok());
    } else if matches!(
        (current.as_str(), next),
        ("waiting_shop", "packing" | "rejected")
            | ("packing", "delivery")
            | ("return_review", "return_accepted" | "return_rejected")
            | ("return_accepted", "return_completed")
    ) {
        if next == "rejected" {
            commerce::restore(&mut tx, id, user.id).await?;
        }
        if next == "delivery" {
            let courier = input
                .courier
                .ok_or_else(|| Error::bad("Choose an expedition"))?;
            let receipt = text(input.receipt.as_deref().unwrap_or(""), 150)?;
            sqlx::query("UPDATE orders SET courier_id=?,receipt=? WHERE id=?")
                .bind(courier)
                .bind(receipt)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO tracking(order_id,note,actor_id) VALUES (?,'Order dispatched',?)",
            )
            .bind(id)
            .bind(user.id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("UPDATE orders SET status=?,actor_id=? WHERE id=?")
            .bind(next)
            .bind(user.id)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        notice(
            &mut tx,
            buyer,
            &format!("Order #{id}: {}", next.replace('_', " ")),
        )
        .await?;
    } else {
        return Err(Error::conflict("That order action is unavailable"));
    }
    tx.commit().await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct Reply {
    body: String,
}
async fn reply(
    State(app): State<App>,
    user: User,
    Path((shop, id)): Path<(i64, i64)>,
    Json(input): Json<Reply>,
) -> Result {
    member(&app, &user, shop, false).await?;
    sqlx::query("UPDATE reviews r JOIN products p ON p.id=r.product_id SET r.reply=?,r.actor_id=? WHERE r.id=? AND p.shop_id=? AND NOT r.deleted").bind(text(&input.body,4000)?).bind(user.id).bind(id).bind(shop).execute(&app.db).await?;
    Ok(ok())
}
const SHOP_OWNED_ACCOUNT: &str = "NOT u.admin AND NOT EXISTS(SELECT 1 FROM shops owned WHERE owned.owner_id=u.id) AND NOT EXISTS(SELECT 1 FROM members other WHERE other.user_id=u.id AND other.shop_id<>m.shop_id)";

async fn team(State(app): State<App>, user: User, Path(shop): Path<i64>) -> Result {
    member(&app, &user, shop, true).await?;
    Ok(Json(json!(team_records(&app, shop, None).await?)))
}

async fn team_member(
    State(app): State<App>,
    user: User,
    Path((shop, employee)): Path<(i64, i64)>,
) -> Result {
    member(&app, &user, shop, true).await?;
    team_records(&app, shop, Some(employee))
        .await?
        .pop()
        .map(Json)
        .ok_or_else(Error::missing)
}

async fn team_records(app: &App, shop: i64, employee: Option<i64>) -> Result<Vec<Value>> {
    let sql = format!(
        "SELECT JSON_OBJECT('id',u.id,'name',u.name,'email',u.email,'role',IF(s.owner_id=u.id,'Owner','Employee'),'active',m.active,'suspended',u.suspended,'can_reset_password',IF({SHOP_OWNED_ACCOUNT},1,0)) FROM members m JOIN users u ON u.id=m.user_id JOIN shops s ON s.id=m.shop_id WHERE m.shop_id="
    );
    let mut query = sqlx::QueryBuilder::<MySql>::new(sql);
    query.push_bind(shop);
    if let Some(employee) = employee {
        query.push(" AND m.user_id=").push_bind(employee);
    }
    query.push(" ORDER BY u.id");
    decode_rows(query.build_query_scalar().fetch_all(&app.db).await?)
}

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum EmployeeChange {
    Access { active: bool },
    Password { password: String },
}

async fn update_employee(
    State(app): State<App>,
    user: User,
    Path((shop, employee)): Path<(i64, i64)>,
    Json(input): Json<EmployeeChange>,
) -> Result {
    member(&app, &user, shop, true).await?;
    let mut tx = app.db.begin().await?;
    let sql = format!(
        "SELECT ({SHOP_OWNED_ACCOUNT}) FROM members m JOIN users u ON u.id=m.user_id JOIN shops s ON s.id=m.shop_id JOIN members owner ON owner.shop_id=s.id AND owner.user_id=s.owner_id WHERE m.shop_id=? AND m.user_id=? AND s.owner_id=? AND m.user_id<>s.owner_id AND owner.active AND NOT s.suspended FOR UPDATE"
    );
    let can_reset: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .bind(shop)
        .bind(employee)
        .bind(user.id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::missing)?;
    match input {
        EmployeeChange::Access { active } => {
            sqlx::query("UPDATE members SET active=?,updated_by=? WHERE shop_id=? AND user_id=? AND active<>?")
                .bind(active).bind(user.id).bind(shop).bind(employee).bind(active)
                .execute(&mut *tx).await?;
        }
        EmployeeChange::Password { password } => {
            if !can_reset {
                return Err(Error::forbidden());
            }
            let password = auth::hash(password).await?;
            auth::change_password(&mut tx, employee, &password).await?;
            sqlx::query("UPDATE members SET updated_by=?,updated_at=UTC_TIMESTAMP(6) WHERE shop_id=? AND user_id=?")
                .bind(user.id).bind(shop).bind(employee).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(ok())
}

#[derive(Deserialize)]
struct Account {
    name: String,
    email: String,
    password: String,
}
async fn create_account(app: &App, input: Account, shop: Option<i64>) -> Result {
    let password = auth::hash(input.password).await?;
    let mut tx = app.db.begin().await?;
    let id = inserted(
        sqlx::query("INSERT INTO users(name,email,password,admin) VALUES (?,?,'!',?)")
            .bind(text(&input.name, 120)?)
            .bind(auth::email(&input.email)?)
            .bind(shop.is_none())
            .execute(&mut *tx)
            .await?,
    )?;
    auth::save_password(&mut tx, id, &password).await?;
    if let Some(shop) = shop {
        sqlx::query("INSERT INTO members(shop_id,user_id) VALUES (?,?)")
            .bind(shop)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
async fn add_team(
    State(app): State<App>,
    user: User,
    Path(shop): Path<i64>,
    Json(input): Json<Account>,
) -> Result {
    member(&app, &user, shop, true).await?;
    create_account(&app, input, Some(shop)).await
}
async fn add_admin(State(app): State<App>, user: User, Json(input): Json<Account>) -> Result {
    user.admin()?;
    create_account(&app, input, None).await
}
async fn shop_dash(
    State(app): State<App>,
    user: User,
    Path(shop): Path<i64>,
    Query(f): Query<Filter>,
) -> Result {
    member(&app, &user, shop, false).await?;
    let (format, days) = period(f.sort.as_deref());
    let sales = series(&app, format, days, shop, 0).await?;
    let stock = decode_rows(
        sqlx::query_scalar(
            "SELECT JSON_OBJECT('label',health,'value',COUNT(*))
             FROM (
                SELECT CASE
                    WHEN p.status='pending' THEN 'On review'
                    WHEN p.status<>'active' THEN 'Inactive'
                    ELSE (
                        SELECT CASE
                            WHEN COALESCE(SUM(v.stock),0)=0 THEN 'Out of stock'
                            WHEN SUM(v.stock)<=GREATEST(1,COALESCE(sales.sold,0)/4) THEN 'Low stock'
                            ELSE 'Healthy'
                        END
                        FROM variants v WHERE v.product_id=p.id AND v.enabled
                    )
                END AS health
                FROM products p FORCE INDEX (product_shop_status_id)
                LEFT JOIN (
                    SELECT l.product_id,SUM(l.quantity) sold
                    FROM orders o JOIN order_lines l ON l.order_id=o.id
                    JOIN products sold_product ON sold_product.id=l.product_id AND sold_product.shop_id=?
                    WHERE o.completed_at>=UTC_TIMESTAMP()-INTERVAL 28 DAY
                    GROUP BY l.product_id
                ) sales ON sales.product_id=p.id
                WHERE p.shop_id=?
             ) inventory GROUP BY health",
        )
        .bind(shop)
        .bind(shop)
        .fetch_all(&app.db)
        .await?,
    )?;
    Ok(Json(
        json!({"sales":sales,"stock":stock,"orders":orders_list(&app,shop,f,true).await?}),
    ))
}
const TAXONOMY: &str = "SELECT JSON_OBJECT('id',id,'name',name,'kind',kind,'status',status,'created_at',created_at) FROM taxonomy";
const ADMIN_PRODUCT_LIMIT: u32 = 100;
const ADMIN_SHOP_LIMIT: u32 = 32;

async fn admin_products(
    app: &App,
    search: Option<&str>,
    status: &str,
    sort: &'static str,
) -> Result<Vec<String>> {
    let search = search.map(str::trim).filter(|q| !q.is_empty());
    let ids = match search {
        Some(q)
            if matches!(status, "" | "active")
                && q.chars().any(char::is_alphanumeric)
                && !q.contains(['%', '_', '\\']) =>
        {
            admin_product_candidates(app, q, status, sort).await?
        }
        _ => None,
    };
    let mut query = product_page_query(if ids.is_some() || status.is_empty() {
        "PRIMARY"
    } else {
        "status"
    });
    match (ids, search) {
        (Some(ids), _) => {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            query.push(" WHERE p.id IN (");
            let mut values = query.separated(",");
            for id in ids {
                values.push_bind(id);
            }
            values.push_unseparated(")");
        }
        (None, Some(q)) => {
            admin_product_substring_filter(&mut query, q);
        }
        (None, None) => {
            query.push(" WHERE TRUE");
        }
    }
    // Check the current status in MariaDB before returning moderation rows.
    if !status.is_empty() {
        query.push(" AND p.status=").push_bind(status);
    }
    product_page(&app.db, query, sort, ADMIN_PRODUCT_LIMIT).await
}

fn admin_product_substring_filter(query: &mut sqlx::QueryBuilder<MySql>, q: &str) {
    query.push(" STRAIGHT_JOIN shops s FORCE INDEX (PRIMARY) ON s.id=p.shop_id WHERE CONCAT_WS(' ',p.name,s.name) LIKE ")
        .push_bind(format!("%{q}%"));
}

async fn chronological_admin_product_ids(
    db: &MySqlPool,
    q: &str,
    status: &str,
    sort: &'static str,
) -> Result<Option<Vec<i64>>> {
    const ER_STATEMENT_TIMEOUT: u16 = 1969;

    // Give broad matches a chance to fill a chronological page without scanning the whole catalog.
    let mut query = sqlx::QueryBuilder::<MySql>::new(
        "SET STATEMENT max_statement_time=0.12 FOR SELECT p.id FROM products p FORCE INDEX (",
    );
    query
        .push(if status.is_empty() {
            "PRIMARY"
        } else {
            "status"
        })
        .push(")");
    admin_product_substring_filter(&mut query, q);
    if !status.is_empty() {
        query.push(" AND p.status=").push_bind(status);
    }
    query
        .push(" ORDER BY p.id ")
        .push(sort)
        .push(" LIMIT ")
        .push_bind(ADMIN_PRODUCT_LIMIT);
    match query.build_query_scalar::<i64>().fetch_all(db).await {
        Ok(ids) if ids.len() == ADMIN_PRODUCT_LIMIT as usize => Ok(Some(ids)),
        Ok(_) => Ok(None),
        Err(sqlx::Error::Database(error))
            if error
                .try_downcast_ref::<MySqlDatabaseError>()
                .is_some_and(|error| error.number() == ER_STATEMENT_TIMEOUT) =>
        {
            Ok(None)
        }
        Err(error) => Err(error.into()),
    }
}

async fn admin_product_candidates(
    app: &App,
    q: &str,
    status: &str,
    sort: &'static str,
) -> Result<Option<Vec<i64>>> {
    let Some(mut ids) = app
        .search
        .admin_product_ids(q, ADMIN_PRODUCT_LIMIT as usize)
        .await
    else {
        return Ok(None);
    };
    if ids.len() > ADMIN_PRODUCT_LIMIT as usize {
        if let Some(page) = chronological_admin_product_ids(&app.db, q, status, sort).await? {
            return Ok(Some(page));
        }
        ids.truncate(ADMIN_PRODUCT_LIMIT as usize);
    }
    let pattern = format!("%{q}%");
    let shops: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM shops WHERE suspended OR name LIKE ? LIMIT ?")
            .bind(&pattern)
            .bind(ADMIN_SHOP_LIMIT + 1)
            .fetch_all(&app.db)
            .await?;
    if shops.len() > ADMIN_SHOP_LIMIT as usize {
        return Ok(None);
    }

    // The storefront omits nonactive listings and products from suspended shops.
    // Shop-name matches also need SQL because the storefront indexes product names separately.
    if status.is_empty() {
        let mut query = sqlx::QueryBuilder::<MySql>::new(
            "SELECT id FROM products FORCE INDEX (status)
             WHERE status IN ('pending','inactive','rejected') AND name LIKE ",
        );
        query
            .push_bind(&pattern)
            .push(" ORDER BY id ")
            .push(sort)
            .push(" LIMIT ")
            .push_bind(ADMIN_PRODUCT_LIMIT);
        ids.extend(query.build_query_scalar::<i64>().fetch_all(&app.db).await?);
    }
    if !shops.is_empty() {
        let mut query = sqlx::QueryBuilder::<MySql>::new(
            "SELECT p.id FROM products p FORCE INDEX (shop_id)
             STRAIGHT_JOIN shops s ON s.id=p.shop_id WHERE p.shop_id IN (",
        );
        let mut shop_ids = query.separated(",");
        for id in shops {
            shop_ids.push_bind(id);
        }
        shop_ids.push_unseparated(")");
        if !status.is_empty() {
            query.push(" AND p.status='active'");
        }
        query
            .push(" AND (p.name LIKE ")
            .push_bind(&pattern)
            .push(" OR s.name LIKE ")
            .push_bind(&pattern)
            .push(") ORDER BY p.id ")
            .push(sort)
            .push(" LIMIT ")
            .push_bind(ADMIN_PRODUCT_LIMIT);
        ids.extend(query.build_query_scalar::<i64>().fetch_all(&app.db).await?);
    }
    Ok(Some(ids))
}

async fn admin_dash(State(app): State<App>, user: User, Query(f): Query<Filter>) -> Result {
    user.admin()?;
    let (format, days) = period(f.status.as_deref());
    let users=decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('label',DATE_FORMAT(created_at,?), 'value',COUNT(DISTINCT user_id)) FROM sessions WHERE created_at>=UTC_TIMESTAMP()-INTERVAL ? DAY GROUP BY DATE_FORMAT(created_at,?) ORDER BY DATE_FORMAT(created_at,?)").bind(format).bind(days).bind(format).bind(format).fetch_all(&app.db).await?)?;
    let sales = series(&app, format, days, 0, 0).await?;
    let pattern = format!("%{}%", f.q.unwrap_or_default());
    let sort = if f.sort.as_deref() == Some("newest") {
        "DESC"
    } else {
        "ASC"
    };
    let mut query = product_page_query("status");
    query
        .push(" WHERE p.status='pending' AND p.name LIKE ")
        .push_bind(&pattern);
    let products = decode_rows(product_page(&app.db, query, sort, 15).await?)?;
    let taxonomy = decode_rows(
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "{TAXONOMY} WHERE status='pending' AND name LIKE ? ORDER BY id {sort} LIMIT 15"
        )))
        .bind(pattern)
        .fetch_all(&app.db)
        .await?,
    )?;
    Ok(Json(
        json!({"users":users,"sales":sales,"products":products,"taxonomy":taxonomy}),
    ))
}
async fn admin_taxonomy(state: State<App>, user: User, query: Query<Filter>) -> Result {
    admin_list(state, user, Path("taxonomy".into()), query).await
}
async fn admin_couriers(state: State<App>, user: User, query: Query<Filter>) -> Result {
    admin_list(state, user, Path("couriers".into()), query).await
}
async fn admin_list(
    State(app): State<App>,
    user: User,
    Path(section): Path<String>,
    Query(f): Query<Filter>,
) -> Result {
    user.admin()?;
    let pattern = format!("%{}%", f.q.as_deref().unwrap_or(""));
    let status = f.status.unwrap_or_default();
    let sort = if f.sort.as_deref() == Some("oldest") {
        "ASC"
    } else {
        "DESC"
    };
    let values=match section.as_str() {
        "products" => {
            admin_products(&app, f.q.as_deref().filter(|q| !q.is_empty()), &status, sort).await?
        }
        "taxonomy"=>sqlx::query_scalar(sqlx::AssertSqlSafe(format!("{TAXONOMY} WHERE name LIKE ? AND (?='' OR status=?) ORDER BY id {sort} LIMIT 100"))).bind(&pattern).bind(&status).bind(&status).fetch_all(&app.db).await?,
        "users"=>sqlx::query_scalar("SELECT JSON_OBJECT('id',id,'name',name,'email',email,'admin',admin,'suspended',suspended) FROM users WHERE CONCAT_WS(' ',name,email) LIKE ? ORDER BY id DESC LIMIT 100").bind(&pattern).fetch_all(&app.db).await?,
        "shops"=>sqlx::query_scalar("SELECT JSON_OBJECT('id',s.id,'name',s.name,'owner',u.name,'suspended',s.suspended) FROM shops s JOIN users u ON u.id=s.owner_id WHERE s.name LIKE ? ORDER BY s.id DESC LIMIT 100").bind(&pattern).fetch_all(&app.db).await?,
        "couriers"=>sqlx::query_scalar("SELECT JSON_OBJECT('id',id,'name',name,'url',url,'logo',logo) FROM couriers WHERE name LIKE ? ORDER BY id").bind(&pattern).fetch_all(&app.db).await?,
        "payments"=>sqlx::query_scalar("SELECT JSON_OBJECT('id',c.id,'checkout_no',c.checkout_no,'name',u.name,'total',c.total,'payment',c.payment,'created_at',c.created_at) FROM checkouts c JOIN users u ON u.id=c.user_id WHERE CONCAT_WS(' ',c.checkout_no,u.name) LIKE ? AND (?='' OR c.payment=?) ORDER BY c.created_at DESC,c.id DESC LIMIT 100").bind(&pattern).bind(&status).bind(&status).fetch_all(&app.db).await?,
        "reviews"=>sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT JSON_OBJECT('id',r.id,'name',p.name,'user',u.name,'shop',s.name,'body',r.body,'rating',r.rating,'created_at',r.created_at,'deleted',r.deleted) FROM reviews r JOIN users u ON u.id=r.user_id JOIN products p ON p.id=r.product_id JOIN shops s ON s.id=p.shop_id WHERE r.deleted=? AND CONCAT_WS(' ',r.body,p.name) LIKE ? AND (?=0 OR s.id=?) AND (?='' OR u.email=?) ORDER BY r.id {sort} LIMIT 100"))).bind(status=="hidden").bind(&pattern).bind(f.shop.unwrap_or(0)).bind(f.shop.unwrap_or(0)).bind(f.category.as_deref().unwrap_or("")).bind(f.category.as_deref().unwrap_or("")).fetch_all(&app.db).await?,
        _=>return Err(Error::missing()),
    };
    Ok(Json(json!(decode_rows(values)?)))
}
#[derive(Deserialize)]
struct Moderation {
    status: String,
    note: Option<String>,
    replacement: Option<i64>,
}
async fn moderate_product(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Moderation>,
) -> Result {
    user.admin()?;
    let note = match input.status.as_str() {
        "active" => "",
        "inactive" | "rejected" => text(input.note.as_deref().unwrap_or(""), 1000)?,
        _ => return Err(Error::bad("Choose a valid status")),
    };
    let mut tx = app.db.begin().await?;
    let (status, shop, brand, category): (String, i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT status,shop_id,brand_id,category_id FROM products WHERE id=? FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if input.status == "active" {
        if status != "pending" && status != "active" {
            return Err(Error::conflict("The merchant must resubmit this product"));
        }
        for term in [brand, category].into_iter().flatten() {
            let valid: bool =
                sqlx::query_scalar("SELECT status='approved' FROM taxonomy WHERE id=? FOR UPDATE")
                    .bind(term)
                    .fetch_one(&mut *tx)
                    .await?;
            if !valid {
                return Err(Error::conflict(
                    "Approve or replace this product's brand and category first",
                ));
            }
        }
    }
    if input.status == "rejected" && status != "pending" && status != "rejected" {
        return Err(Error::conflict("Only submitted products can be rejected"));
    }
    sqlx::query("UPDATE products SET status=?,note=?,actor_id=? WHERE id=?")
        .bind(&input.status)
        .bind(note)
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM shops WHERE id=?")
        .bind(shop)
        .fetch_one(&mut *tx)
        .await?;
    notice(
        &mut tx,
        owner,
        &format!("Product #{id}: {}. {note}", input.status)
            .chars()
            .take(500)
            .collect::<String>(),
    )
    .await?;
    tx.commit().await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct Term {
    name: String,
    kind: String,
}
async fn save_taxonomy(State(app): State<App>, user: User, Json(input): Json<Term>) -> Result {
    user.admin()?;
    if !matches!(input.kind.as_str(), "brand" | "category") {
        return Err(Error::bad("Choose brand or category"));
    }
    let id = inserted(
        sqlx::query("INSERT INTO taxonomy(kind,name,status) VALUES (?,?,'approved')")
            .bind(input.kind)
            .bind(text(&input.name, 120)?)
            .execute(&app.db)
            .await?,
    )?;
    Ok(Json(json!({"id":id})))
}
async fn moderate_taxonomy(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Moderation>,
) -> Result {
    user.admin()?;
    if !matches!(input.status.as_str(), "approved" | "rejected") {
        return Err(Error::bad("Choose approve or reject"));
    }
    let mut tx = app.db.begin().await?;
    let (kind, status): (String, String) =
        sqlx::query_as("SELECT kind,status FROM taxonomy WHERE id=? FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if status == "approved" && input.status == "rejected" {
        return Err(Error::conflict("Only proposed taxonomy can be rejected"));
    }
    if let Some(replacement) = input.replacement {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM taxonomy WHERE id=? AND kind=? AND status='approved')",
        )
        .bind(replacement)
        .bind(&kind)
        .fetch_one(&mut *tx)
        .await?;
        if !valid || replacement == id {
            return Err(Error::bad(
                "Choose a different approved term of the same type",
            ));
        }
        let col = if kind == "brand" {
            "brand_id"
        } else {
            "category_id"
        };
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE products SET {col}=?,actor_id=? WHERE {col}=?"
        )))
        .bind(replacement)
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE taxonomy SET status=? WHERE id=?")
        .bind(input.status)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ok())
}
async fn delete_review(State(app): State<App>, user: User, Path(id): Path<i64>) -> Result {
    user.admin()?;
    sqlx::query("UPDATE reviews SET deleted=TRUE,actor_id=? WHERE id=? AND NOT deleted")
        .bind(user.id)
        .bind(id)
        .execute(&app.db)
        .await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct UserUpdate {
    suspended: Option<bool>,
    password: Option<String>,
}
async fn update_user(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<UserUpdate>,
) -> Result {
    user.admin()?;
    if id == user.id && input.suspended == Some(true) {
        return Err(Error::bad("You cannot suspend your current account"));
    }
    let password = match input.password {
        Some(p) => Some(auth::hash(p).await?),
        None => None,
    };
    let mut tx = app.db.begin().await?;
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM users WHERE id=? AND email<>'checkout-expiry@system.tosca.invalid' FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(suspended) = input.suspended {
        sqlx::query("UPDATE users SET suspended=? WHERE id=?")
            .bind(suspended)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        if suspended {
            sqlx::query("UPDATE sessions SET revoked=TRUE WHERE user_id=?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    if let Some(password) = password {
        auth::change_password(&mut tx, id, &password).await?;
    }
    tx.commit().await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct Suspended {
    suspended: bool,
}
async fn suspend_shop(
    State(app): State<App>,
    user: User,
    Path(id): Path<i64>,
    Json(input): Json<Suspended>,
) -> Result {
    user.admin()?;
    sqlx::query("UPDATE shops SET suspended=? WHERE id=?")
        .bind(input.suspended)
        .bind(id)
        .execute(&app.db)
        .await?;
    Ok(ok())
}
#[derive(Deserialize)]
struct Courier {
    id: Option<i64>,
    name: String,
    url: String,
    logo: String,
}
async fn save_courier(State(app): State<App>, user: User, Json(input): Json<Courier>) -> Result {
    user.admin()?;
    if !input.logo.is_empty() {
        catalog::media(&input.logo, true)?;
    }
    if !input.url.is_empty()
        && !(input.url.starts_with("https://") || input.url.starts_with("http://"))
    {
        return Err(Error::bad("Enter an HTTP or HTTPS tracking URL"));
    }
    if input.url.len() > 255 {
        return Err(Error::bad("Tracking URL is too long"));
    }
    if let Some(id) = input.id {
        if id == 1 {
            return Err(Error::bad("The built-in self-shipping option is fixed"));
        }
        sqlx::query("UPDATE couriers SET name=?,logo=?,url=? WHERE id=?")
            .bind(text(&input.name, 120)?)
            .bind(input.logo)
            .bind(input.url)
            .bind(id)
            .execute(&app.db)
            .await?;
    } else {
        sqlx::query("INSERT INTO couriers(name,logo,url) VALUES (?,?,?)")
            .bind(text(&input.name, 120)?)
            .bind(input.logo)
            .bind(input.url)
            .execute(&app.db)
            .await?;
    }
    Ok(ok())
}
