use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::Context;
use fastembed::{AutoDevicePolicy, EmbeddingModel, TextEmbedding, TextInitOptions};
use sqlx::{Connection, MySql, MySqlConnection, MySqlPool, QueryBuilder};
use tokio::sync::{Mutex, OnceCell};

pub const MODEL: &str = "e5_small_v1";
pub const DIMENSIONS: usize = 384;
const QUERY_EMBEDDING_BUDGET: Duration = Duration::from_secs(1);
const CPU_BATCH_SIZE: u32 = 8;
const GPU_BATCH_SIZE: u32 = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Cpu,
    DirectMl,
}

impl FromStr for Device {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "cpu" => Ok(Self::Cpu),
            "directml" => Ok(Self::DirectMl),
            _ => anyhow::bail!("EMBEDDINGS_DEVICE must be cpu or directml"),
        }
    }
}

impl Device {
    fn batch_size(self) -> u32 {
        match self {
            Self::Cpu => CPU_BATCH_SIZE,
            Self::DirectMl => GPU_BATCH_SIZE,
        }
    }
}

fn load_model(device: Device) -> anyhow::Result<TextEmbedding> {
    let options = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
        .with_intra_threads(2)
        .with_max_length(384)
        .with_cache_dir(std::env::temp_dir().join("tosca-embedding-models"))
        .with_show_download_progress(false);
    let options = match device {
        Device::Cpu => options.with_auto_device(AutoDevicePolicy::PreferCPU),
        #[cfg(windows)]
        Device::DirectMl => options.with_execution_providers(vec![
            ort::ep::DirectML::default()
                .with_device_filter(ort::ep::directml::DeviceFilter::Gpu)
                .build()
                .error_on_failure(),
        ]),
        #[cfg(not(windows))]
        Device::DirectMl => anyhow::bail!("DirectML requires Windows"),
    };
    Ok(TextEmbedding::try_new(options)?)
}

struct LoadedModel {
    session: Arc<Mutex<TextEmbedding>>,
    device: Device,
}

#[derive(Clone)]
pub struct Embedder(Option<Arc<OnceCell<LoadedModel>>>);

impl Embedder {
    pub fn new(enabled: bool) -> Self {
        Self(enabled.then(|| Arc::new(OnceCell::new())))
    }

    pub fn enabled(&self) -> bool {
        self.0.is_some()
    }

    async fn initialize(&self, device: Device) -> anyhow::Result<()> {
        let Some(cell) = &self.0 else { return Ok(()) };
        cell.get_or_try_init(|| async {
            tokio::task::spawn_blocking(move || {
                let (model, device) = match load_model(device) {
                    Ok(model) => (model, device),
                    Err(error) if device == Device::DirectMl => {
                        tracing::warn!(%error, "DirectML initialization failed; using CPU embeddings");
                        (load_model(Device::Cpu)?, Device::Cpu)
                    }
                    Err(error) => return Err(error),
                };
                tracing::info!(
                    model = MODEL,
                    ?device,
                    catalog_batch_size = device.batch_size(),
                    "local text embedder ready"
                );
                Ok(LoadedModel {
                    session: Arc::new(Mutex::new(model)),
                    device,
                })
            })
            .await?
        })
        .await?;
        Ok(())
    }

    pub async fn query(&self, query: &str) -> Option<Vec<f32>> {
        let model = &self.0.as_ref()?.get()?.session;
        let query = format!("query: {query}");
        // Give concurrent searches a chance to use the model before falling back.
        let vectors = tokio::time::timeout(QUERY_EMBEDDING_BUDGET, async {
            let mut model = Arc::clone(model).lock_owned().await;
            tokio::task::spawn_blocking(move || model.embed([query], Some(1))).await
        })
        .await
        .inspect_err(|_| tracing::debug!("query embedding timed out; using keyword search"))
        .ok()?
        .ok()?
        .ok()?;
        vectors
            .into_iter()
            .next()
            .filter(|vector| valid_vector(vector))
    }
}

fn valid_vector(vector: &[f32]) -> bool {
    vector.len() == DIMENSIONS && vector.iter().all(|v| v.is_finite())
}

pub async fn enqueue(conn: &mut MySqlConnection, products: &[i64]) -> anyhow::Result<()> {
    let mut query = QueryBuilder::<MySql>::new(
        "INSERT INTO product_embeddings(product_id,model,fingerprint,next_attempt_at)
         SELECT id,",
    );
    query.push_bind(MODEL).push(
        ",SHA2(passage,256),UTC_TIMESTAMP(6)
         FROM recommendation_product_text WHERE id IN (",
    );
    let mut params = query.separated(",");
    for id in products {
        params.push_bind(*id);
    }
    params.push_unseparated(")");
    query.push(
        " ON DUPLICATE KEY UPDATE
         embedding=IF(model<>VALUES(model) OR fingerprint<>VALUES(fingerprint),NULL,embedding),
         next_attempt_at=IF(model<>VALUES(model) OR fingerprint<>VALUES(fingerprint)
                            OR (embedding IS NULL AND next_attempt_at IS NULL),UTC_TIMESTAMP(6),next_attempt_at),
         model=VALUES(model),fingerprint=VALUES(fingerprint)",
    );
    query.build().execute(conn).await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Pending {
    product_id: i64,
    fingerprint: String,
    passage: Option<String>,
}

async fn publish(pool: &MySqlPool, embedder: &Embedder) -> anyhow::Result<bool> {
    let Some(model) = embedder.0.as_ref().and_then(|value| value.get()) else {
        return Ok(false);
    };
    let batch_size = model.device.batch_size();
    let mut connection = MySqlConnection::connect_with(&pool.connect_options()).await?;
    let owns_job: Option<i32> =
        sqlx::query_scalar("SELECT GET_LOCK(CONCAT('tosca-embeddings-',MD5(DATABASE())),0)")
            .fetch_one(&mut connection)
            .await?;
    if owns_job != Some(1) {
        return Ok(false);
    }
    let jobs: Vec<Pending> = sqlx::query_as(
        "SELECT e.product_id,e.fingerprint,
            (SELECT t.passage FROM recommendation_product_text t
             WHERE t.id=e.product_id AND SHA2(t.passage,256)=e.fingerprint) AS passage
         FROM product_embeddings e FORCE INDEX (embedding_due)
         WHERE e.model=? AND e.next_attempt_at<=UTC_TIMESTAMP(6)
         ORDER BY e.next_attempt_at,e.product_id LIMIT ?",
    )
    .bind(MODEL)
    .bind(batch_size)
    .fetch_all(&mut connection)
    .await?;
    if jobs.is_empty() {
        return Ok(false);
    }
    let passages: Vec<_> = jobs.iter().filter_map(|job| job.passage.clone()).collect();
    let mut vectors = if passages.is_empty() {
        Vec::new().into_iter()
    } else {
        let count = passages.len();
        let device = model.device;
        let mut session = Arc::clone(&model.session).lock_owned().await;
        let vectors = tokio::task::spawn_blocking(move || {
            match session.embed(&passages, Some(batch_size as usize)) {
                Err(error) if device == Device::DirectMl && count > CPU_BATCH_SIZE as usize => {
                    tracing::warn!(%error, count, "DirectML batch failed; retrying in smaller batches");
                    session.embed(passages, Some(CPU_BATCH_SIZE as usize))
                }
                result => result,
            }
        })
        .await??;
        anyhow::ensure!(vectors.len() == count, "Embedding batch size mismatch");
        vectors.into_iter()
    };
    for job in jobs {
        let vector = job.passage.as_ref().and_then(|_| vectors.next());
        let mut tx = connection.begin().await?;
        // Read and lock the current versioned rows before updating the queue row.
        // MariaDB can hide those rows when the view is read inside UPDATE.
        let current: Option<String> = sqlx::query_scalar(
            "SELECT SHA2(passage,256) FROM recommendation_product_text WHERE id=? FOR UPDATE",
        )
        .bind(job.product_id)
        .fetch_optional(&mut *tx)
        .await?;
        if current.as_deref() != Some(job.fingerprint.as_str()) {
            // enqueue() will requeue this row if the product becomes eligible again.
            sqlx::query(
                "UPDATE product_embeddings SET next_attempt_at=NULL
                 WHERE product_id=? AND model=? AND fingerprint=? AND next_attempt_at IS NOT NULL",
            )
            .bind(job.product_id)
            .bind(MODEL)
            .bind(&job.fingerprint)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            continue;
        }
        let Some(vector) = vector else {
            tx.commit().await?;
            continue;
        };
        if !valid_vector(&vector) {
            sqlx::query(
                "UPDATE product_embeddings SET next_attempt_at=UTC_TIMESTAMP(6)+INTERVAL 60 SECOND
                 WHERE product_id=? AND model=? AND fingerprint=?",
            )
            .bind(job.product_id)
            .bind(MODEL)
            .bind(&job.fingerprint)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            continue;
        }
        let updated = sqlx::query(
            "UPDATE product_embeddings SET embedding=?,next_attempt_at=NULL
             WHERE product_id=? AND model=? AND fingerprint=? AND next_attempt_at IS NOT NULL",
        )
        .bind(sqlx::types::Json(vector))
        .bind(job.product_id)
        .bind(MODEL)
        .bind(&job.fingerprint)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() > 0 {
            sqlx::query(
                "INSERT INTO search_sync_jobs(entity_type,entity_id) VALUES ('product',?)
                 ON DUPLICATE KEY UPDATE revision=revision+1,updated_at=CURRENT_TIMESTAMP(6)",
            )
            .bind(job.product_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    connection.close().await?;
    Ok(true)
}

pub async fn worker(pool: MySqlPool, embedder: Embedder, recommendations: bool, device: Device) {
    if recommendations {
        tokio::spawn(prune_signals(pool.clone()));
    }
    loop {
        match embedder
            .initialize(device)
            .await
            .context("loading multilingual E5 model")
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %format_args!("{error:#}"), "semantic embeddings unavailable; continuing with lexical search");
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
    }
    loop {
        let delay = match publish(&pool, &embedder).await {
            Ok(true) => Duration::from_millis(100),
            Ok(false) => Duration::from_secs(3),
            Err(error) => {
                tracing::warn!(%error, "catalog embeddings paused; pending work will be retried");
                Duration::from_secs(10)
            }
        };
        tokio::time::sleep(delay).await;
    }
}

async fn prune_signals(pool: MySqlPool) {
    loop {
        let result = tokio::try_join!(
            sqlx::query(
                "DELETE FROM recommendation_signals WHERE signal_day<UTC_DATE()-INTERVAL 90 DAY LIMIT 1000",
            ).execute(&pool),
            sqlx::query(
                "DELETE FROM recommendation_guest_views WHERE view_hour<UTC_TIMESTAMP()-INTERVAL 7 DAY LIMIT 1000",
            ).execute(&pool),
        );
        let delay = match result {
            Ok((account, guest))
                if account.rows_affected() == 1000 || guest.rows_affected() == 1000 =>
            {
                Duration::from_secs(2)
            }
            Ok(_) => Duration::from_secs(3600),
            Err(error) => {
                tracing::warn!(%error, "recommendation signal retention cleanup failed");
                Duration::from_secs(60)
            }
        };
        tokio::time::sleep(delay).await;
    }
}
