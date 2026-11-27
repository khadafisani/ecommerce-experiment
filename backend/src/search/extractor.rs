use std::time::Duration;

use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::Instant;

use crate::{json, setting};

const CPU_BATCH_SIZE: usize = 32;
const PRIMARY_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_PRIMARY_AFTER: Duration = Duration::from_secs(30);

pub(super) struct Extractor {
    client: Client,
    primary: String,
    fallback: Option<String>,
    retry_primary_at: Option<Instant>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Task {
    Product,
    Variant,
    Query,
}

#[derive(Default, Deserialize)]
pub(super) struct Entities {
    pub(super) brand: Option<String>,
    pub(super) product: Option<String>,
    pub(super) product_type: Option<String>,
    pub(super) compatibility: Option<String>,
    pub(super) option: Option<String>,
}

#[derive(Deserialize)]
struct Response {
    results: Vec<Entities>,
}

impl Extractor {
    pub(super) fn new() -> anyhow::Result<Self> {
        let primary = setting("SEARCH_GLINER_URL", "http://127.0.0.1:11439");
        let fallback = setting("SEARCH_GLINER_FALLBACK_URL", "http://127.0.0.1:11435");
        let primary = primary.trim_end_matches('/');
        let fallback = fallback.trim_end_matches('/');
        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(3))
                .build()?,
            primary: format!("{primary}/extract"),
            fallback: (!fallback.is_empty() && fallback != primary)
                .then(|| format!("{fallback}/extract")),
            retry_primary_at: None,
        })
    }

    pub(super) async fn extract(
        &mut self,
        task: Task,
        texts: &[String],
    ) -> anyhow::Result<Vec<Entities>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let Some(fallback) = &self.fallback else {
            return self
                .request(&self.primary, REQUEST_TIMEOUT, task, texts)
                .await;
        };
        if self.retry_primary_at.is_none_or(|at| Instant::now() >= at) {
            match self
                .request(&self.primary, PRIMARY_TIMEOUT, task, texts)
                .await
            {
                Ok(results) => {
                    if self.retry_primary_at.take().is_some() {
                        tracing::info!("primary GLiNER worker recovered");
                    }
                    return Ok(results);
                }
                Err(error) => {
                    tracing::warn!(error = %format_args!("{error:#}"), "primary GLiNER worker unavailable; using CPU fallback");
                    self.retry_primary_at = Some(Instant::now() + RETRY_PRIMARY_AFTER);
                }
            }
        }
        let mut results = Vec::with_capacity(texts.len());
        for batch in texts.chunks(CPU_BATCH_SIZE) {
            results.extend(
                self.request(fallback, REQUEST_TIMEOUT, task, batch)
                    .await
                    .context("CPU GLiNER fallback failed")?,
            );
        }
        Ok(results)
    }

    async fn request(
        &self,
        endpoint: &str,
        timeout: Duration,
        task: Task,
        texts: &[String],
    ) -> anyhow::Result<Vec<Entities>> {
        let response: Response = self
            .client
            .post(endpoint)
            .timeout(timeout)
            .json(&json!({"task":task,"texts":texts}))
            .send()
            .await
            .context("requesting local GLiNER extraction")?
            .error_for_status()?
            .json()
            .await
            .context("decoding GLiNER response")?;
        anyhow::ensure!(
            response.results.len() == texts.len(),
            "GLiNER returned {} results for {} inputs",
            response.results.len(),
            texts.len()
        );
        Ok(response.results)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
    use serde_json::Value;

    use super::*;

    #[tokio::test]
    async fn uses_bounded_cpu_batches_and_recovers_primary() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/primary",
                post(|State(attempts): State<Arc<AtomicUsize>>| async move {
                    if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                        return Err(StatusCode::SERVICE_UNAVAILABLE);
                    }
                    Ok(Json(json!({"results":[{"product":"primary"}]})))
                }),
            )
            .route(
                "/cpu",
                post(|Json(body): Json<Value>| async move {
                    let inputs = body["texts"].as_array().unwrap();
                    assert!(inputs.len() <= CPU_BATCH_SIZE);
                    let results: Vec<_> =
                        inputs.iter().map(|text| json!({"product": text})).collect();
                    Json(json!({"results":results}))
                }),
            )
            .with_state(attempts.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let mut extractor = Extractor {
            client: Client::new(),
            primary: format!("http://{address}/primary"),
            fallback: Some(format!("http://{address}/cpu")),
            retry_primary_at: None,
        };
        let inputs: Vec<_> = (0..65).map(|index| format!("title {index}")).collect();
        let results = extractor.extract(Task::Product, &inputs).await.unwrap();
        assert!(
            results
                .iter()
                .zip(&inputs)
                .all(|(row, text)| row.product.as_ref() == Some(text))
        );
        extractor
            .extract(Task::Product, &inputs[..1])
            .await
            .unwrap();
        assert_eq!(attempts.load(Ordering::Relaxed), 1);

        extractor.retry_primary_at = Some(Instant::now());
        let recovered = extractor
            .extract(Task::Product, &inputs[..1])
            .await
            .unwrap();
        assert_eq!(recovered[0].product.as_deref(), Some("primary"));
        assert!(extractor.retry_primary_at.is_none());
        server.abort();
    }
}
