use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Seek},
    path::Path,
    time::Instant,
};

use anyhow::{Context, ensure};
use csv::{Position, Reader, StringRecord};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{Connection, MySql, MySqlConnection, MySqlPool, QueryBuilder};

use crate::search::{self, Search};

const CHUNK_SIZE: usize = 5_000;
const PROGRESS_INTERVAL: u64 = 50_000;
// The marketplace stores whole rupiah. Use a fixed rate so reruns are reproducible.
const RUPIAH_PER_CENT: i64 = 160;
const NO_PRICE_RUPIAH: i64 = 10_000;

pub(super) struct Options {
    limit: u64,
}

impl Options {
    pub(super) fn parse(args: &[String]) -> anyhow::Result<Self> {
        let limit = match args {
            [] => u64::MAX,
            [flag, value] if flag == "--limit" => value.parse()?,
            _ => anyhow::bail!("Usage: tosca-market --import-amazon [--limit TOTAL]"),
        };
        ensure!(limit > 0, "The import limit must be positive");
        Ok(Self { limit })
    }
}

#[derive(Deserialize)]
struct Category {
    id: u32,
    category_name: String,
}

#[derive(Deserialize)]
struct SourceProduct<'a> {
    asin: &'a str,
    title: &'a str,
    category_id: u32,
    price: &'a str,
    #[serde(rename = "listPrice")]
    list_price: &'a str,
}

struct Product {
    name: String,
    description: String,
    category_id: i64,
    price: i64,
    stock: i32,
}

#[derive(sqlx::FromRow)]
struct Progress {
    fingerprint: Vec<u8>,
    shop_id: i64,
    next_byte: u64,
    imported: u64,
}

pub(super) async fn run(db: &MySqlPool, search: &Search, options: Options) -> anyhow::Result<()> {
    let mut conn = MySqlConnection::connect_with(&db.connect_options()).await?;
    let locked: Option<i32> =
        sqlx::query_scalar("SELECT GET_LOCK(CONCAT('tosca-amazon-', MD5(DATABASE())), 0)")
            .fetch_one(&mut conn)
            .await?;
    ensure!(
        locked == Some(1),
        "An Amazon catalog import is already running"
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let categories =
        fs::read(root.join("amazon_categories.csv")).context("Reading amazon_categories.csv")?;
    let mut products =
        File::open(root.join("amazon_products.csv")).context("Opening amazon_products.csv")?;
    let fingerprint = fingerprint(&mut products, &categories)?;
    let mut reader = Reader::from_reader(products);
    let headers = reader.headers()?.clone();
    let mut progress = catalog(&mut conn, &fingerprint).await?;
    let ids = category_ids(&mut conn, &categories).await?;
    if progress.next_byte > 0 {
        let mut position = Position::new();
        position
            .set_byte(progress.next_byte)
            .set_record(progress.imported + 1);
        reader.seek(position)?;
    }

    let started = Instant::now();
    let mut record = StringRecord::new();
    let mut chunk = Vec::with_capacity(CHUNK_SIZE);
    while progress.imported < options.limit {
        let remaining = options.limit - progress.imported;
        for _ in 0..remaining.min(CHUNK_SIZE as u64) {
            if !reader.read_record(&mut record)? {
                break;
            }
            let product = record.deserialize(Some(&headers))?;
            chunk.push(map_product(product, &ids).with_context(|| {
                format!("Reading product at byte {}", reader.position().byte())
            })?);
        }
        if chunk.is_empty() {
            break;
        }
        let next_byte = reader.position().byte();
        let imported = progress.imported + chunk.len() as u64;
        insert_chunk(&mut conn, progress.shop_id, &chunk, next_byte, imported)
            .await
            .with_context(|| format!("Committing Amazon products through row {imported}"))?;
        if imported / PROGRESS_INTERVAL > progress.imported / PROGRESS_INTERVAL {
            tracing::info!(imported, elapsed = ?started.elapsed(), "Amazon products committed");
        }
        progress.imported = imported;
        chunk.clear();
    }

    tracing::info!(
        imported = progress.imported,
        "Amazon products committed; syncing Meilisearch"
    );
    search::sync_bulk(db, search).await?;
    tracing::info!(
        imported = progress.imported,
        "Amazon catalog search sync complete"
    );
    conn.close().await?;
    Ok(())
}

fn fingerprint(file: &mut File, categories: &[u8]) -> anyhow::Result<[u8; 32]> {
    let mut hash = Sha256::new();
    hash.update(Sha256::digest(categories));
    let mut buffer = vec![0; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    file.rewind()?;
    Ok(hash.finalize().into())
}

async fn catalog(conn: &mut MySqlConnection, fingerprint: &[u8]) -> anyhow::Result<Progress> {
    if let Some(progress) = sqlx::query_as::<_, Progress>(
        "SELECT fingerprint,shop_id,next_byte,imported FROM amazon_catalog_import WHERE id=1",
    )
    .fetch_optional(&mut *conn)
    .await?
    {
        ensure!(
            progress.fingerprint == fingerprint,
            "The source CSV files differ from the files used for this database's import"
        );
        return Ok(progress);
    }
    let mut tx = conn.begin().await?;
    let owner = sqlx::query(
        "INSERT INTO users(email,name,password,suspended)
         VALUES ('amazon-catalog@seed.tosca.invalid','Amazon Catalog','!',TRUE)",
    )
    .execute(&mut *tx)
    .await?
    .last_insert_id();
    let shop_id = i64::try_from(
        sqlx::query("INSERT INTO shops(name,owner_id) VALUES ('Amazon Catalog Benchmark',?)")
            .bind(owner)
            .execute(&mut *tx)
            .await?
            .last_insert_id(),
    )?;
    sqlx::query("INSERT INTO amazon_catalog_import(id,fingerprint,shop_id) VALUES (1,?,?)")
        .bind(fingerprint)
        .bind(shop_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Progress {
        fingerprint: fingerprint.to_vec(),
        shop_id,
        next_byte: 0,
        imported: 0,
    })
}

async fn category_ids(conn: &mut MySqlConnection, csv: &[u8]) -> anyhow::Result<HashMap<u32, i64>> {
    let mut tx = conn.begin().await?;
    let mut ids = HashMap::new();
    for category in Reader::from_reader(csv).deserialize::<Category>() {
        let Category { id, category_name } = category?;
        sqlx::query(
            "INSERT INTO taxonomy(kind,name,status) VALUES ('category',?,'approved')
             ON DUPLICATE KEY UPDATE id=id",
        )
        .bind(&category_name)
        .execute(&mut *tx)
        .await?;
        let taxonomy_id: i64 =
            sqlx::query_scalar("SELECT id FROM taxonomy WHERE kind='category' AND name=?")
                .bind(category_name)
                .fetch_one(&mut *tx)
                .await?;
        ids.insert(id, taxonomy_id);
    }
    tx.commit().await?;
    Ok(ids)
}

fn usd_cents(value: &str) -> anyhow::Result<i64> {
    if value.is_empty() {
        return Ok(0);
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let cents = match fraction.len() {
        0 => 0,
        1 => fraction.parse::<i64>()? * 10,
        2 => fraction.parse::<i64>()?,
        _ => anyhow::bail!("Expected a dollar amount with at most two decimal places"),
    };
    let dollars: i64 = whole.parse()?;
    ensure!(
        dollars >= 0 && (0..100).contains(&cents),
        "Invalid dollar amount"
    );
    dollars
        .checked_mul(100)
        .and_then(|total| total.checked_add(cents))
        .context("Dollar amount is too large")
}

fn map_product(
    source: SourceProduct<'_>,
    categories: &HashMap<u32, i64>,
) -> anyhow::Result<Product> {
    ensure!(
        source.asin.len() == 10 && source.asin.bytes().all(|byte| byte.is_ascii_alphanumeric()),
        "Invalid Amazon ASIN"
    );
    let category_id = *categories
        .get(&source.category_id)
        .context("Unknown Amazon category")?;
    let title = source.title.trim();
    ensure!(!title.is_empty(), "Empty title for {}", source.asin);
    let name: String = title.chars().take(180).collect();
    let mut description = format!("Amazon ASIN: {}", source.asin);
    if name.len() < title.len() {
        description.push('\n');
        description.push_str(title);
    }
    let listed_cents = usd_cents(source.price)?;
    let cents = if listed_cents > 0 {
        listed_cents
    } else {
        usd_cents(source.list_price)?
    };
    let price = if cents == 0 {
        NO_PRICE_RUPIAH
    } else {
        cents
            .checked_mul(RUPIAH_PER_CENT)
            .filter(|price| *price <= 1_000_000_000)
            .context("Converted price exceeds the marketplace price limit")?
    };
    Ok(Product {
        name,
        description,
        category_id,
        price,
        stock: if cents == 0 { 0 } else { 30 },
    })
}

async fn insert_chunk(
    conn: &mut MySqlConnection,
    shop_id: i64,
    products: &[Product],
    next_byte: u64,
    imported: u64,
) -> anyhow::Result<()> {
    let mut tx = conn.begin().await?;
    let owner: i64 = sqlx::query_scalar("SELECT owner_id FROM shops WHERE id=?")
        .bind(shop_id)
        .fetch_one(&mut *tx)
        .await?;
    let mut rows = QueryBuilder::<MySql>::new(
        "INSERT INTO products(shop_id,name,description,category_id,status,actor_id) ",
    );
    rows.push_values(products, |mut row, product| {
        row.push_bind(shop_id)
            .push_bind(&product.name)
            .push_bind(&product.description)
            .push_bind(product.category_id)
            .push("'active'")
            .push_bind(owner);
    });
    rows.push(" RETURNING id, description");
    let inserted: Vec<(i64, String)> = rows
        .build_query_as()
        .persistent(false)
        .fetch_all(&mut *tx)
        .await?;
    let by_description: HashMap<_, _> = products
        .iter()
        .map(|product| (&product.description, product))
        .collect();
    ensure!(
        inserted.len() == products.len() && by_description.len() == products.len(),
        "Unexpected or duplicate products in the import chunk"
    );

    let variants = inserted
        .iter()
        .map(|(id, description)| {
            by_description
                .get(description)
                .map(|product| (id, product))
                .context("Unable to match a saved product to its Amazon row")
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut rows = QueryBuilder::<MySql>::new(
        "INSERT INTO variants(product_id,name,price,stock,grams,actor_id) ",
    );
    rows.push_values(variants, |mut row, (id, product)| {
        row.push_bind(id)
            .push("'Standard'")
            .push_bind(product.price)
            .push_bind(product.stock)
            .push("500")
            .push_bind(owner);
    });
    rows.build().persistent(false).execute(&mut *tx).await?;
    // The product and variant triggers enqueue search updates in this transaction.
    sqlx::query("UPDATE amazon_catalog_import SET next_byte=?, imported=? WHERE id=1")
        .bind(next_byte)
        .bind(imported)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
