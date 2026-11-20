use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use anyhow::Context;
use sqlx::MySqlPool;
use stoolap::{Database, Value};

use super::{CANDIDATES, DAY, Filters, Signal, SignalKind, Sort, TTL};
use crate::embeddings::{DIMENSIONS, MODEL};

const MAX_RANKINGS: i64 = 512;
const EXPLORE_TIER_SIZE: usize = CANDIDATES / 2;

#[derive(Default)]
pub(super) struct Cache(Mutex<Option<Projection>>);

struct Projection {
    db: Database,
    _directory: tempfile::TempDir,
    refreshed: Instant,
    catalog_digest: [u8; 32],
}

#[cfg(test)]
pub(super) enum Lookup {
    Hit(Vec<i64>),
    Miss,
    Unavailable,
}

#[derive(sqlx::FromRow)]
pub(super) struct CatalogRow {
    id: i64,
    category_id: Option<i64>,
    shop_id: i64,
    price: Option<i64>,
    popularity: i64,
    trend_views: i64,
    embedding: Option<sqlx::types::Json<Vec<f32>>>,
}

pub(super) async fn read_catalog(pool: &MySqlPool) -> anyhow::Result<Vec<CatalogRow>> {
    Ok(sqlx::query_as(
        "SELECT p.id,p.category_id,p.shop_id,
            (SELECT MIN(v.price) FROM variants v WHERE v.product_id=p.id AND v.enabled AND v.stock>0) AS price,
            COALESCE(pop.sales,0) AS popularity, COALESCE(trend.views,0) AS trend_views, e.embedding
         FROM products p JOIN shops s ON s.id=p.shop_id
         LEFT JOIN recommendation_product_text t ON t.id=p.id
         LEFT JOIN product_embeddings e ON e.product_id=p.id AND e.model=? AND e.fingerprint=SHA2(t.passage,256)
         LEFT JOIN (
            SELECT l.product_id,COUNT(DISTINCT c.id) AS sales
            FROM checkouts c JOIN orders o ON o.checkout_id=c.id JOIN order_lines l ON l.order_id=o.id
            WHERE c.payment='paid' AND c.created_at>=UTC_TIMESTAMP()-INTERVAL 30 DAY
              AND o.status NOT IN ('rejected','cancelled')
            GROUP BY l.product_id
         ) pop ON pop.product_id=p.id
         LEFT JOIN (
            SELECT product_id,CAST(SUM(views) AS SIGNED) AS views
            FROM (
                SELECT product_id,COUNT(*) AS views FROM recommendation_signals
                WHERE kind='view' AND seen_at>=UTC_TIMESTAMP()-INTERVAL 7 DAY GROUP BY product_id
                UNION ALL
                SELECT product_id,SUM(views) FROM recommendation_guest_views
                WHERE view_hour>=UTC_TIMESTAMP()-INTERVAL 7 DAY GROUP BY product_id
            ) activity GROUP BY product_id
         ) trend ON trend.product_id=p.id
         WHERE p.status='active' AND NOT s.suspended ORDER BY p.id",
    ).bind(MODEL).fetch_all(pool).await?)
}

impl Cache {
    pub(super) async fn refresh(
        self: &Arc<Self>,
        rows: Vec<CatalogRow>,
        started: Instant,
    ) -> anyhow::Result<()> {
        let cache = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            use sha2::{Digest, Sha256};
            let mut digest = Sha256::new();
            for row in &rows {
                for value in [row.id, row.category_id.unwrap_or(0), row.shop_id, row.price.unwrap_or(0), row.popularity, row.trend_views] {
                    digest.update(value.to_le_bytes());
                }
                if let Some(vector) = &row.embedding {
                    digest.update((vector.len() as u64).to_le_bytes());
                    for value in &vector.0 {
                        digest.update(value.to_le_bytes());
                    }
                } else {
                    digest.update(0_u64.to_le_bytes());
                }
            }
            let digest = digest.finalize().into();
            let mut current = cache.0.lock().map_err(|_| anyhow::anyhow!("recommendation cache lock poisoned"))?;
            if current.as_ref().is_some_and(|db| db.refreshed > started) {
                return Ok(());
            }
            let initial = current.is_none();
            let projection = match current.as_mut() {
                Some(projection) => projection,
                None => current.insert(Projection::open(started)?),
            };
            if projection.catalog_digest != digest {
                let mut tx = projection.db.begin()?;
                tx.execute("DELETE FROM ranked_products", ())?;
                tx.execute("DELETE FROM catalog", ())?;
                let insert = projection.db.prepare(
                    "INSERT INTO catalog(id,category_id,shop_id,price,popularity,trend_views,embedding)
                     VALUES ($1,$2,$3,$4,$5,$6,$7)",
                )?;
                for row in rows {
                    let vector = row.embedding
                        .filter(|vector| vector.len() == DIMENSIONS && vector.iter().all(|n| n.is_finite()))
                        .map_or_else(Value::null_unknown, |vector| Value::vector(vector.0));
                    insert.execute_in(&mut tx, (row.id, row.category_id, row.shop_id, row.price, row.popularity, row.trend_views, vector))?;
                }
                tx.commit()?;
                projection.catalog_digest = digest;
            } else {
                projection.prune()?;
            }
            projection.refreshed = started;
            if initial {
                tracing::info!("private Stoolap recommendation cache ready");
            }
            Ok(())
        }).await?
    }

    #[cfg(test)]
    pub(super) async fn lookup(self: &Arc<Self>, key: String) -> anyhow::Result<Lookup> {
        let cache = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let Ok(current) = cache.0.try_lock() else {
                return Ok(Lookup::Unavailable);
            };
            let Some(projection) = current.as_ref().filter(|p| p.refreshed.elapsed() < TTL) else {
                return Ok(Lookup::Unavailable);
            };
            Ok(match projection.lookup(&key)? {
                Some(ids) => Lookup::Hit(ids),
                None => Lookup::Miss,
            })
        })
        .await?
    }

    pub(super) async fn rank(
        self: &Arc<Self>,
        key: String,
        filters: Filters,
        signals: Vec<Signal>,
        now: i64,
    ) -> anyhow::Result<Option<Vec<i64>>> {
        let cache = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let current = cache
                .0
                .lock()
                .map_err(|_| anyhow::anyhow!("recommendation cache lock poisoned"))?;
            let Some(projection) = current.as_ref().filter(|p| p.refreshed.elapsed() < TTL) else {
                return Ok(None);
            };
            if let Some(ids) = projection.lookup(&key)? {
                return Ok(Some(ids));
            }
            let ids = projection.calculate(&filters, signals, now)?;
            projection.prune()?;
            let count: i64 = projection
                .db
                .query_one("SELECT COUNT(*) FROM ranked_products", ())?;
            if count < MAX_RANKINGS {
                projection.db.execute(
                    "INSERT INTO ranked_products(cache_key,expires,ids) VALUES ($1,$2,$3)",
                    (
                        key,
                        now + TTL.as_secs() as i64,
                        serde_json::to_string(&ids)?,
                    ),
                )?;
            }
            Ok(Some(ids))
        })
        .await?
    }
}

impl Projection {
    fn open(started: Instant) -> anyhow::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("tosca-recommendations-")
            .tempdir()?;
        let db = Database::open(&format!("file://{}", directory.path().display()))
            .context("opening private Stoolap cache")?;
        db.execute(
            "CREATE TABLE catalog (id INTEGER PRIMARY KEY, category_id INTEGER, shop_id INTEGER NOT NULL,
             price INTEGER, popularity INTEGER NOT NULL, trend_views INTEGER NOT NULL, embedding VECTOR(384))", (),
        )?;
        db.execute(
            "CREATE INDEX catalog_category ON catalog(category_id,id)",
            (),
        )?;
        db.execute("CREATE INDEX catalog_shop ON catalog(shop_id,id)", ())?;
        db.execute(
            "CREATE TABLE ranked_products (id INTEGER PRIMARY KEY AUTO_INCREMENT,
             cache_key TEXT NOT NULL UNIQUE, expires INTEGER NOT NULL, ids TEXT NOT NULL)",
            (),
        )?;
        db.execute(
            "CREATE INDEX ranking_expiry ON ranked_products(expires)",
            (),
        )?;
        Ok(Self {
            db,
            _directory: directory,
            refreshed: started - TTL,
            catalog_digest: [0; 32],
        })
    }

    fn lookup(&self, key: &str) -> anyhow::Result<Option<Vec<i64>>> {
        let data: Option<String> = self.db.query_opt(
            "SELECT ids FROM ranked_products WHERE cache_key=$1 AND expires>$2",
            (key, chrono::Utc::now().timestamp()),
        )?;
        data.map(|data| serde_json::from_str(&data))
            .transpose()
            .map_err(Into::into)
    }

    fn prune(&self) -> anyhow::Result<()> {
        self.db.execute(
            "DELETE FROM ranked_products WHERE expires<=$1",
            (chrono::Utc::now().timestamp(),),
        )?;
        Ok(())
    }

    fn calculate(
        &self,
        filters: &Filters,
        signals: Vec<Signal>,
        now: i64,
    ) -> anyhow::Result<Vec<i64>> {
        let explore = matches!(filters.sort, Sort::Explore);
        let mut weights: BTreeMap<i64, (f64, f64)> = BTreeMap::new();
        for signal in signals {
            let age_days = (now - signal.at).max(0) as f64 / DAY as f64;
            let weight = match signal.kind {
                SignalKind::View => 1.0,
                SignalKind::Cart => 3.0,
                SignalKind::Checkout => 6.0,
            } * 2_f64.powf(-age_days / 14.0);
            let (product, views) = weights.entry(signal.product_id).or_default();
            *product += weight;
            if signal.kind == SignalKind::View {
                *views += weight;
            }
        }
        if let Some(id) = filters.product_id {
            let (product, views) = weights.entry(id).or_default();
            *product += 6.0;
            *views += 6.0;
        }
        let mut categories = HashMap::<i64, f64>::new();
        let mut centroid = vec![0.0; DIMENSIONS];
        if !weights.is_empty() {
            let parameters: Vec<_> = weights.keys().copied().map(Value::integer).collect();
            let sql = format!(
                "SELECT id,category_id,embedding FROM catalog WHERE id IN ({})",
                vec!["?"; parameters.len()].join(",")
            );
            for row in self.db.query(&sql, parameters)? {
                let row = row?;
                let id: i64 = row.get(0)?;
                let (product, views) = weights[&id];
                if let Some(category) = row.get::<Option<i64>>(1)? {
                    *categories.entry(category).or_default() +=
                        if explore { product } else { views };
                }
                if let Some(vector) = row.get_value(2).and_then(Value::as_vector_f32) {
                    for (sum, component) in centroid.iter_mut().zip(vector) {
                        *sum += component * product as f32;
                    }
                }
            }
        }
        let mut parameters = Vec::new();
        let mut sql = if centroid.iter().any(|&n| n != 0.0)
            && matches!(filters.sort, Sort::Relevance | Sort::Explore)
        {
            parameters.push(Value::vector(centroid));
            String::from(
                "SELECT id,category_id,popularity,trend_views,VEC_DISTANCE_COSINE(embedding,?) FROM catalog WHERE price IS NOT NULL",
            )
        } else {
            String::from(
                "SELECT id,category_id,popularity,trend_views,NULL FROM catalog WHERE price IS NOT NULL",
            )
        };
        for (value, clause) in [
            (filters.product_id, " AND id<>?"),
            (filters.category_id, " AND category_id=?"),
            (filters.shop_id, " AND shop_id=?"),
            (filters.min_price, " AND price>=?"),
            (filters.max_price, " AND price<=?"),
        ] {
            if let Some(value) = value {
                sql.push_str(clause);
                parameters.push(Value::integer(value));
            }
        }
        let order = match filters.sort {
            Sort::Newest => Some("id DESC"),
            Sort::PriceAsc => Some("price ASC,id DESC"),
            Sort::PriceDesc => Some("price DESC,id DESC"),
            Sort::Relevance if weights.is_empty() => Some("popularity DESC,id DESC"),
            Sort::Relevance | Sort::Explore => None,
        };
        if let Some(order) = order {
            sql.push_str(&format!(" ORDER BY {order} LIMIT {CANDIDATES}"));
            return self
                .db
                .query(&sql, parameters)?
                .map(|row| Ok(row?.get(0)?))
                .collect();
        }
        let max_product = weights.values().map(|value| value.0).fold(1.0, f64::max);
        let max_category = categories.values().copied().fold(1.0, f64::max);
        let max_popularity = if explore {
            1.0
        } else {
            let count: Option<i64> = self
                .db
                .query_one("SELECT MAX(popularity) FROM catalog", ())?;
            (count.unwrap_or(0) as f64).ln_1p().max(1.0)
        };
        let mut scored = Vec::new();
        let mut trending = Vec::new();
        for row in self.db.query(&sql, parameters)? {
            let row = row?;
            let id: i64 = row.get(0)?;
            let category = row
                .get::<Option<i64>>(1)?
                .and_then(|id| categories.get(&id))
                .copied()
                .unwrap_or(0.0);
            let affinity = weights.get(&id).map_or(0.0, |value| value.0);
            if explore {
                let views = row.get::<i64>(3)?;
                if views > 0 {
                    trending.push((id, views));
                }
                if category == 0.0 && affinity == 0.0 {
                    continue;
                }
            }
            let distance: Option<f64> = row.get(4)?;
            let semantic = distance.map_or(0.0, |distance| (1.0 - distance).clamp(0.0, 1.0));
            let mut score =
                0.4 * category / max_category + 0.3 * affinity / max_product + 0.2 * semantic;
            if !explore {
                score += 0.1 * (row.get::<i64>(2)? as f64).ln_1p() / max_popularity;
            }
            scored.push((id, score));
        }
        scored.sort_unstable_by(|a, b| b.1.total_cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        if explore {
            let mut ids: Vec<_> = scored
                .into_iter()
                .take(EXPLORE_TIER_SIZE)
                .map(|(id, _)| id)
                .collect();
            let preferred: HashSet<_> = ids.iter().copied().collect();
            trending.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
            ids.extend(
                trending
                    .into_iter()
                    .map(|(id, _)| id)
                    .filter(|id| !preferred.contains(id))
                    .take(EXPLORE_TIER_SIZE),
            );
            return Ok(ids);
        }
        Ok(scored
            .into_iter()
            .take(CANDIDATES)
            .map(|(id, _)| id)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(
        id: i64,
        category_id: i64,
        price: Option<i64>,
        popularity: i64,
        component: usize,
    ) -> CatalogRow {
        let mut embedding = vec![0.0; DIMENSIONS];
        embedding[component] = 1.0;
        CatalogRow {
            id,
            category_id: Some(category_id),
            shop_id: 1,
            price,
            popularity,
            trend_views: 0,
            embedding: Some(sqlx::types::Json(embedding)),
        }
    }

    #[tokio::test]
    async fn ranks_and_filters_the_current_snapshot() -> anyhow::Result<()> {
        let cache = Arc::new(Cache::default());
        cache
            .refresh(
                vec![
                    product(1, 10, Some(15), 0, 0),
                    product(2, 10, Some(20), 1, 0),
                    product(3, 20, Some(10), 100, 1),
                    product(4, 10, None, 50, 0),
                ],
                Instant::now(),
            )
            .await?;
        let now = chrono::Utc::now().timestamp();
        let cold = cache
            .rank("guest".into(), Filters::default(), Vec::new(), now)
            .await?;
        assert_eq!(cold, Some(vec![3, 2, 1]));
        let filters = Filters {
            product_id: Some(1),
            ..Filters::default()
        };
        let similar = cache
            .rank("detail".into(), filters, Vec::new(), now)
            .await?;
        assert_eq!(similar, Some(vec![2, 3]));
        assert!(
            matches!(cache.lookup("detail".into()).await?, Lookup::Hit(ids) if ids == vec![2, 3])
        );
        let filters = Filters {
            category_id: Some(10),
            max_price: Some(19),
            sort: Sort::PriceAsc,
            ..Filters::default()
        };
        assert_eq!(
            cache
                .rank("filter".into(), filters, Vec::new(), now)
                .await?,
            Some(vec![1])
        );
        cache
            .refresh(vec![product(3, 20, Some(10), 100, 1)], Instant::now())
            .await?;
        assert!(matches!(cache.lookup("detail".into()).await?, Lookup::Miss));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_an_expired_snapshot() -> anyhow::Result<()> {
        let cache = Arc::new(Cache::default());
        cache.refresh(Vec::new(), Instant::now() - TTL).await?;
        assert!(matches!(
            cache.lookup("expired".into()).await?,
            Lookup::Unavailable
        ));
        Ok(())
    }

    #[tokio::test]
    async fn explore_ranks_preferences_before_trends_and_leaves_general_items_for_the_tail()
    -> anyhow::Result<()> {
        let cache = Arc::new(Cache::default());
        let now = chrono::Utc::now().timestamp();
        cache
            .refresh(
                vec![
                    product(1, 10, Some(15), 0, 0),
                    CatalogRow {
                        trend_views: 2,
                        ..product(2, 10, Some(15), 0, 0)
                    },
                    CatalogRow {
                        trend_views: 90,
                        ..product(3, 20, Some(10), 0, 1)
                    },
                    product(4, 20, Some(10), 500, 1),
                    CatalogRow {
                        trend_views: 100,
                        ..product(5, 20, None, 0, 1)
                    },
                ],
                Instant::now(),
            )
            .await?;
        let filters = Filters {
            sort: Sort::Explore,
            ..Filters::default()
        };
        let cold = cache
            .rank("cold-explore".into(), filters.clone(), Vec::new(), now)
            .await?;
        assert_eq!(cold, Some(vec![3, 2]));
        let view = vec![Signal {
            product_id: 1,
            kind: SignalKind::View,
            at: now,
        }];
        let ranked = cache
            .rank("personal-explore".into(), filters, view, now)
            .await?;
        assert_eq!(ranked, Some(vec![1, 2, 3]));
        Ok(())
    }
}
