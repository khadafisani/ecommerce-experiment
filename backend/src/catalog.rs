use super::*;
use crate::{auth::User, uploads};
use axum::{
    extract::{Multipart, Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;

const CARD_SELECT: &str = "SELECT JSON_OBJECT('id',p.id,'name',p.name,'image',p.image,'shop_id',s.id,'shop',s.name,'category',c.name,'lowest_price',(SELECT MIN(v.price) FROM variants v WHERE v.product_id=p.id AND v.enabled))";
const VISIBLE_CATALOG: &str =
    " LEFT JOIN taxonomy c ON c.id=p.category_id WHERE p.status='active' AND NOT s.suspended";
pub fn routes() -> Router<App> {
    Router::new()
        .route("/health", get(health))
        .route("/products", get(products))
        .route("/products/{id}", get(product))
        .route("/search", get(crate::search::query))
        .route("/categories", get(categories))
        .route("/couriers", get(couriers))
        .route("/upload", post(upload))
}
async fn health(State(app): State<App>) -> Result {
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&app.db)
        .await?;
    Ok(Json(json!({"mariadb":version})))
}
#[derive(Deserialize, Default)]
pub struct Filter {
    pub q: Option<String>,
    pub cursor: Option<i64>,
    pub category: Option<String>,
    pub shop: Option<i64>,
    pub status: Option<String>,
    pub sort: Option<String>,
}
pub async fn rows(db: &MySqlPool, query: &'static str) -> Result<Vec<Value>> {
    let values: Vec<String> = sqlx::query_scalar(query).fetch_all(db).await?;
    decode_rows(values)
}
pub fn decode_rows(values: Vec<String>) -> Result<Vec<Value>> {
    values
        .into_iter()
        .map(|row| serde_json::from_str(&row).map_err(Error::internal))
        .collect()
}
pub fn page(mut items: Vec<Value>) -> Json<Value> {
    let cursor = if items.len() > 20 {
        items.pop();
        items.last().and_then(|item| item["id"].as_i64())
    } else {
        None
    };
    Json(json!({"items":items,"cursor":cursor}))
}
async fn products(State(app): State<App>, Query(f): Query<Filter>) -> Result {
    let oldest = f.sort.as_deref() == Some("oldest");
    let cursor = f.cursor.unwrap_or(if oldest { 0 } else { i64::MAX });
    let category = f.category.as_deref().filter(|name| !name.is_empty());
    let shop = f.shop.filter(|id| *id != 0);
    let index = match (shop, category) {
        (_, Some(_)) => "product_category_status_id",
        (Some(_), _) => "product_shop_status_id",
        _ => "status",
    };
    let mut query = ordered_cards(index, CARD_SELECT);
    if let Some(category) = category {
        query
            .push(" AND p.category_id=(SELECT id FROM taxonomy WHERE kind='category' AND name=")
            .push_bind(category)
            .push(")");
    }
    if let Some(shop) = shop {
        query.push(" AND p.shop_id=").push_bind(shop);
    }
    let (cursor_sql, sort_sql) = if oldest {
        (" AND p.id>", " ORDER BY p.id ASC LIMIT 21")
    } else {
        (" AND p.id<", " ORDER BY p.id DESC LIMIT 21")
    };
    query.push(cursor_sql).push_bind(cursor).push(sort_sql);
    let values = query.build_query_scalar().fetch_all(&app.db).await?;
    Ok(page(decode_rows(values)?))
}

pub(super) fn ordered_cards(
    index: &'static str,
    select: impl Into<String>,
) -> sqlx::QueryBuilder<MySql> {
    let mut query = sqlx::QueryBuilder::new(select);
    query
        .push(" FROM products p FORCE INDEX (")
        .push(index)
        .push(") STRAIGHT_JOIN shops s FORCE INDEX (PRIMARY) ON s.id=p.shop_id")
        .push(VISIBLE_CATALOG);
    query
}

pub async fn explore_tail(
    db: &MySqlPool,
    before: Option<i64>,
    excluded: &[i64],
    limit: u32,
) -> Result<Vec<Value>> {
    // Starting with an ordered product index lets MariaDB stop after filling the page.
    // A shop-first or buffered join materializes and sorts the catalog before LIMIT.
    let mut query = ordered_cards("status", CARD_SELECT);
    if let Some(before) = before {
        query.push(" AND p.id<").push_bind(before);
    }
    if !excluded.is_empty() {
        query.push(" AND p.id NOT IN (");
        let mut ids = query.separated(",");
        for id in excluded {
            ids.push_bind(*id);
        }
        ids.push_unseparated(")");
    }
    query.push(" ORDER BY p.id DESC LIMIT ").push_bind(limit);
    decode_rows(query.build_query_scalar().fetch_all(db).await?)
}

pub async fn variants(app: &App, id: i64, show_disabled: bool) -> Result<Vec<Value>> {
    decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('id',id,'name',name,'subvariant',subvariant,'price',price,'stock',stock,'grams',grams,'enabled',enabled) FROM variants WHERE product_id=? AND (? OR enabled) ORDER BY id")
        .bind(id).bind(show_disabled).fetch_all(&app.db).await?)
}
pub async fn reviews(app: &App, id: i64) -> Result<Vec<Value>> {
    decode_rows(sqlx::query_scalar("SELECT JSON_OBJECT('id',r.id,'user',u.name,'rating',r.rating,'body',r.body,'media',JSON_EXTRACT(r.media,'$'),'reply',r.reply,'created_at',r.created_at) FROM reviews r JOIN users u ON u.id=r.user_id WHERE r.product_id=? AND NOT r.deleted ORDER BY r.id DESC LIMIT 100").bind(id).fetch_all(&app.db).await?)
}
async fn product(State(app): State<App>, Path(id): Path<i64>) -> Result {
    let (raw, shop_id, category_id): (String, i64, Option<i64>) = sqlx::query_as(
        "SELECT JSON_OBJECT('id',p.id,'name',p.name,'description',p.description,'image',p.image,
            'shop',s.name,'shop_id',s.id,'brand',b.name,'category',c.name),s.id,c.id
         FROM products p JOIN shops s ON s.id=p.shop_id
         LEFT JOIN taxonomy b ON b.id=p.brand_id LEFT JOIN taxonomy c ON c.id=p.category_id
         WHERE p.id=? AND p.status='active' AND NOT s.suspended",
    )
    .bind(id)
    .fetch_one(&app.db)
    .await?;
    let mut item: Value = serde_json::from_str(&raw).map_err(Error::internal)?;
    let (variants, reviews, similar) = tokio::try_join!(
        variants(&app, id, false),
        reviews(&app, id),
        similar_products(&app.db, id, shop_id, category_id),
    )?;
    item["variants"] = json!(variants);
    item["reviews"] = json!(reviews);
    item["similar"] = json!(similar);
    Ok(Json(item))
}

async fn similar_products(
    db: &MySqlPool,
    id: i64,
    shop_id: i64,
    category_id: Option<i64>,
) -> Result<Vec<Value>> {
    // The newest eight rows from either index are sufficient to find the newest
    // eight in their union. Fetch card details only after choosing those IDs.
    let query = format!(
        "{CARD_SELECT}
         FROM (
            SELECT id FROM (
                (SELECT p.id FROM products p FORCE INDEX (shop_id)
                 WHERE p.shop_id=? AND p.status='active' AND p.id<>?
                   AND (SELECT s.suspended FROM shops s WHERE s.id=p.shop_id)=0
                 ORDER BY p.id DESC LIMIT 8)
                UNION DISTINCT
                (SELECT p.id FROM products p FORCE INDEX (product_category_status_id)
                 WHERE p.category_id=? AND p.status='active' AND p.id<>?
                   AND (SELECT s.suspended FROM shops s WHERE s.id=p.shop_id)=0
                 ORDER BY p.id DESC LIMIT 8)
            ) candidates ORDER BY id DESC LIMIT 8
         ) related STRAIGHT_JOIN products p FORCE INDEX (PRIMARY) ON p.id=related.id
         JOIN shops s ON s.id=p.shop_id
         {VISIBLE_CATALOG} ORDER BY p.id DESC"
    );
    decode_rows(
        sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .bind(shop_id)
            .bind(id)
            .bind(category_id)
            .bind(id)
            .fetch_all(db)
            .await?,
    )
}
async fn categories(State(app): State<App>) -> Result {
    Ok(Json(json!(rows(&app.db, "SELECT JSON_OBJECT('id',id,'name',name,'kind',kind) FROM taxonomy WHERE status='approved' ORDER BY name").await?)))
}
async fn couriers(State(app): State<App>) -> Result {
    Ok(Json(json!(rows(&app.db,"SELECT JSON_OBJECT('id',id,'name',name,'url',url,'logo',logo) FROM couriers ORDER BY id").await?)))
}
#[derive(Deserialize)]
struct UploadOptions {
    #[serde(default)]
    private: bool,
}
async fn upload(user: User, Query(options): Query<UploadOptions>, mut form: Multipart) -> Result {
    let file = form
        .next_field()
        .await
        .map_err(|_| Error::bad("Invalid upload"))?
        .ok_or_else(|| Error::bad("Choose a file"))?;
    let mime = file.content_type().unwrap_or("").to_owned();
    let bytes = file
        .bytes()
        .await
        .map_err(|_| Error::bad("Invalid upload"))?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err(Error::bad("The maximum file size is 20 MB"));
    }
    let extension = match mime.as_str() {
        "video/mp4" if bytes.get(4..8) == Some(b"ftyp") => "mp4",
        "video/webm" if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) => "webm",
        _ if mime.starts_with("image/") && bytes.len() <= uploads::MAX_IMAGE_BYTES => "avif",
        _ => {
            return Err(Error::bad(
                "Choose an image up to 8 MB or an MP4/WebM video up to 20 MB",
            ));
        }
    };
    let id = uuid::Uuid::new_v4();
    let (directory, url, stem) = if options.private {
        (
            uploads::PRIVATE_DIR,
            RETURN_MEDIA_PREFIX,
            format!("{}-{id}", user.id),
        )
    } else {
        (uploads::PUBLIC_DIR, "/uploads/", id.to_string())
    };
    let name = format!("{stem}.{extension}");
    if extension == "avif" {
        uploads::save_image(directory, stem, bytes).await?;
    } else {
        tokio::fs::write(std::path::Path::new(directory).join(&name), bytes)
            .await
            .map_err(Error::internal)?;
    }
    Ok(Json(json!({"url":format!("{url}{name}")})))
}
pub const RETURN_MEDIA_PREFIX: &str = "/api/return-media/";
pub fn return_media_name(name: &str) -> Option<(i64, &'static str)> {
    let (owner, file) = name.split_once('-')?;
    let (id, extension) = file.split_once('.')?;
    uuid::Uuid::parse_str(id).ok()?;
    let mime = match extension {
        "avif" => "image/avif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => return None,
    };
    let owner: i64 = owner.parse().ok()?;
    if !name.starts_with(&format!("{owner}-")) {
        return None;
    }
    Some((owner, mime))
}

pub fn media(value: &str, image: bool) -> Result<&str> {
    let path = value
        .strip_prefix("/uploads/")
        .ok_or_else(|| Error::bad("Upload the file first"))?;
    let (id, extension) = path
        .split_once('.')
        .ok_or_else(|| Error::bad("Invalid upload"))?;
    if uuid::Uuid::parse_str(id).is_err()
        || !(extension == "avif" || (!image && matches!(extension, "mp4" | "webm")))
    {
        return Err(Error::bad("Invalid upload"));
    }
    Ok(value)
}
pub async fn seed(app: &App) -> anyhow::Result<()> {
    let mut tx = app.db.begin().await?;
    let result = sqlx::query("INSERT IGNORE INTO users(email,name,password) VALUES ('catalog@tosca.invalid','Tosca Catalog','!')").execute(&mut *tx).await?;
    if result.rows_affected() == 0 {
        return Ok(());
    }
    let owner = result.last_insert_id();
    let mut shops = std::collections::HashMap::new();
    for entry in include_str!("../fixtures/catalog.tsv").lines() {
        let fields: Vec<_> = entry.split('\t').collect();
        if fields.len() != 6 {
            anyhow::bail!("Invalid demo catalog row");
        }
        let [name, shop, category, price, slug, description] = fields[..] else {
            unreachable!()
        };
        let shop_id = if let Some(id) = shops.get(shop) {
            *id
        } else {
            let id = sqlx::query("INSERT INTO shops(name,owner_id) VALUES (?,?)")
                .bind(shop)
                .bind(owner)
                .execute(&mut *tx)
                .await?
                .last_insert_id();
            shops.insert(shop, id);
            id
        };
        sqlx::query(
            "INSERT IGNORE INTO taxonomy(kind,name,status) VALUES ('category',?,'approved')",
        )
        .bind(category)
        .execute(&mut *tx)
        .await?;
        let id = sqlx::query("INSERT INTO products(shop_id,name,description,image,category_id,status,actor_id) VALUES (?,?,?,?,(SELECT id FROM taxonomy WHERE kind='category' AND name=?),'active',?)")
            .bind(shop_id).bind(name).bind(description).bind(format!("/catalog/{slug}.jpg")).bind(category).bind(owner).execute(&mut *tx).await?.last_insert_id();
        sqlx::query("INSERT INTO variants(product_id,name,price,stock,grams,actor_id) VALUES (?,'Standard',?,30,500,?)").bind(id).bind(price.parse::<i64>()?).bind(owner).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}
