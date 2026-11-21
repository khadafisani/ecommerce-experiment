mod amazon_import;
mod auth;
mod catalog;
mod commerce;
mod embeddings;
mod operations;
mod recommendations;
mod search;
#[cfg(test)]
mod security_tests;
mod test_seed;
mod uploads;

use axum::{
    Json, Router,
    body::Body,
    extract::{MatchedPath, Request},
    http::{
        self, Extensions, HeaderMap, StatusCode,
        header::{
            ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, REFERRER_POLICY,
            X_CONTENT_TYPE_OPTIONS,
        },
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sqlx::{
    MySql, MySqlPool, Transaction,
    migrate::MigrateDatabase,
    mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlSslMode},
};
use std::{env, sync::Arc};
use tower::{ServiceExt, service_fn};
use tower_http::{
    compression::{
        Compression, CompressionLayer,
        predicate::{Predicate, SizeAbove},
    },
    services::{ServeDir, ServeFile},
};

#[derive(Clone)]
struct App {
    db: MySqlPool,
    secret: Arc<str>,
    search: search::Search,
    recommendations: recommendations::Recommendations,
    tax: i64,
    payment_hold_hours: u32,
    allow_test_accounts: bool,
    app_url: Arc<url::Url>,
}

type Result<T = Json<Value>> = std::result::Result<T, Error>;
struct Error(StatusCode, String);
impl Error {
    fn bad(message: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, message.into())
    }
    fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "Please sign in".into())
    }
    fn forbidden() -> Self {
        Self(StatusCode::FORBIDDEN, "Access denied".into())
    }
    fn missing() -> Self {
        Self(StatusCode::NOT_FOUND, "Not found".into())
    }
    fn conflict(message: &str) -> Self {
        Self(StatusCode::CONFLICT, message.into())
    }
    fn busy() -> Self {
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Please try again shortly".into(),
        )
    }
    fn internal(error: impl std::fmt::Display) -> Self {
        tracing::error!(%error, "request failed");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong. Please try again.".into(),
        )
    }
}
impl From<sqlx::Error> for Error {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::missing(),
            sqlx::Error::Database(ref error) if error.is_unique_violation() => {
                Self::conflict("This record already exists")
            }
            _ => Self::internal(error),
        }
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
fn ok() -> Json<Value> {
    Json(json!({"ok": true}))
}

fn format_database_datetime(value: time::PlainDateTime) -> String {
    let date = value.date();
    let (hour, minute, second, micros) = value.as_hms_micro();
    let datetime = format!("{date}T{hour:02}:{minute:02}:{second:02}");
    // Keep the existing JSON precision for MariaDB timestamps.
    if micros == 0 {
        datetime
    } else if micros.is_multiple_of(1_000) {
        let millis = micros / 1_000;
        format!("{datetime}.{millis:03}")
    } else {
        format!("{datetime}.{micros:06}")
    }
}

fn text(value: &str, max: usize) -> Result<&str> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(Error::bad(format!("Enter between 1 and {max} characters")));
    }
    Ok(value)
}
fn setting(name: &str, default: &str) -> String {
    env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.into())
}
fn inserted(result: sqlx::mysql::MySqlQueryResult) -> Result<i64> {
    i64::try_from(result.last_insert_id()).map_err(Error::internal)
}
async fn notice(tx: &mut Transaction<'_, MySql>, user: i64, message: &str) -> Result<()> {
    sqlx::query("INSERT INTO notifications(user_id,message) VALUES (?,?)")
        .bind(user)
        .bind(message)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
async fn claim(
    tx: &mut Transaction<'_, MySql>,
    actor: i64,
    scope: &str,
    key: &str,
    input: &impl serde::Serialize,
) -> Result<Option<i64>> {
    let key = canonical_request_key(key)?;
    let fingerprint = auth::digest(&serde_json::to_string(input).map_err(Error::internal)?);
    sqlx::query("INSERT INTO idempotency(actor_id,scope,request_key,fingerprint) VALUES (?,?,?,?) ON DUPLICATE KEY UPDATE request_key=request_key")
        .bind(actor).bind(scope).bind(&key).bind(&fingerprint).execute(&mut **tx).await?;
    let (stored, result): (String, Option<i64>) = sqlx::query_as("SELECT fingerprint,result_id FROM idempotency WHERE actor_id=? AND scope=? AND request_key=? FOR UPDATE")
        .bind(actor).bind(scope).bind(key).fetch_one(&mut **tx).await?;
    if stored != fingerprint {
        return Err(Error::conflict(
            "Request key was already used for different details",
        ));
    }
    Ok(result)
}
async fn finish(
    tx: &mut Transaction<'_, MySql>,
    actor: i64,
    scope: &str,
    key: &str,
    id: i64,
) -> Result<()> {
    let key = canonical_request_key(key)?;
    let updated = sqlx::query(
        "UPDATE idempotency SET result_id=? WHERE actor_id=? AND scope=? AND request_key=?",
    )
    .bind(id)
    .bind(actor)
    .bind(scope)
    .bind(key)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(Error::internal("missing idempotency result row"));
    }
    Ok(())
}

fn canonical_request_key(key: &str) -> Result<String> {
    uuid::Uuid::parse_str(key)
        .map(|id| id.to_string())
        .map_err(|_| Error::bad("A valid request key is required"))
}

async fn prevent_api_caching(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .entry(CACHE_CONTROL)
        .or_insert(http::HeaderValue::from_static("no-store"));
    response
}

async fn require_same_origin_write(
    axum::extract::State(app): axum::extract::State<App>,
    request: Request,
    next: Next,
) -> Response {
    use http::{Method, header::ORIGIN};

    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        let headers = request.headers();
        let explicit = headers
            .get("x-tosca-request")
            .is_some_and(|value| value == "1");
        let same_origin = headers.get(ORIGIN).is_none_or(|origin| {
            origin.as_bytes() == app.app_url.origin().ascii_serialization().as_bytes()
        });
        let fetch_site = headers
            .get("sec-fetch-site")
            .is_none_or(|value| value == "same-origin");
        if !explicit || !same_origin || !fetch_site {
            return Error::forbidden().into_response();
        }
    }
    next.run(request).await
}

fn storefront_url(value: &str) -> anyhow::Result<url::Url> {
    let url = url::Url::parse(value)?;
    let local = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    };
    anyhow::ensure!(
        url.has_host()
            && (url.scheme() == "https" || (local && url.scheme() == "http"))
            && url.username().is_empty()
            && url.password().is_none()
            && url.path() == "/"
            && url.query().is_none()
            && url.fragment().is_none(),
        "APP_URL must be the storefront HTTPS origin or a local HTTP origin"
    );
    Ok(url)
}

async fn browser_security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        REFERRER_POLICY,
        http::HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        X_CONTENT_TYPE_OPTIONS,
        http::HeaderValue::from_static("nosniff"),
    );
    if headers
        .get(CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        headers.insert(
            CONTENT_SECURITY_POLICY,
            http::HeaderValue::from_static(
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; img-src 'self' blob: data:; media-src 'self' blob:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'; form-action 'self'",
            ),
        );
    }
    response
}

async fn require_acceptable_encoding(request: Request, next: Next) -> Response {
    let encodings = request.headers().get_all(ACCEPT_ENCODING);
    // Tower HTTP checks for 406 after the handler runs. Probe weighted offers
    // first so a rejected request cannot commit a change before returning 406.
    if encodings
        .iter()
        .any(|value| value.as_bytes().contains(&b';'))
    {
        let mut probe = http::Request::new(());
        for value in encodings {
            probe.headers_mut().append(ACCEPT_ENCODING, value.clone());
        }
        let compression = Compression::new(service_fn(|_| async {
            Ok::<_, std::convert::Infallible>(StatusCode::NO_CONTENT.into_response())
        }));
        let Ok(response) = compression.oneshot(probe).await;
        if response.status() == StatusCode::NOT_ACCEPTABLE {
            return response.map(Body::new);
        }
    }
    next.run(request).await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let args: Vec<_> = env::args().skip(1).collect();
    let (seed_test, amazon_import) = match args.as_slice() {
        [] => (false, None),
        [flag] if flag == "--seed-test" => (true, None),
        [flag, rest @ ..] if flag == "--import-amazon" => {
            (false, Some(amazon_import::Options::parse(rest)?))
        }
        _ => anyhow::bail!("Usage: tosca-market [--seed-test | --import-amazon [--limit TOTAL]]"),
    };
    let url = env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("Set DATABASE_URL in the backend environment"))?;
    let options: MySqlConnectOptions = url.parse()?;
    let local_db = matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]"
    );
    if seed_test || amazon_import.is_some() {
        anyhow::ensure!(
            local_db,
            "Test catalog data can only be added to a local database"
        );
    }
    anyhow::ensure!(
        local_db || matches!(options.get_ssl_mode(), MySqlSslMode::VerifyIdentity),
        "Remote MariaDB requires ssl-mode=VERIFY_IDENTITY in DATABASE_URL"
    );
    if !MySql::database_exists(&url).await? {
        MySql::create_database(&url).await?;
    }
    let db = MySqlPoolOptions::new()
        .max_connections(12)
        .connect_with(options)
        .await?;
    let version: String = sqlx::query_scalar("SELECT VERSION()")
        .fetch_one(&db)
        .await?;
    tracing::info!(%version, "MariaDB connected");
    sqlx::migrate!().run(&db).await?;
    if let Some(options) = amazon_import {
        let search = search::Search::new(false, embeddings::Embedder::new(false))?;
        return amazon_import::run(&db, &search, options).await;
    }
    let secret = setting("JWT_SECRET", "");
    if secret.len() < 32 {
        anyhow::bail!("Set JWT_SECRET to at least 32 bytes in .env");
    }
    let recommendations = recommendations::Recommendations::new();
    let hybrid_search = setting("SEMANTIC_SEARCH_ENABLED", "false") == "true";
    let embedder =
        embeddings::Embedder::new(!seed_test && (hybrid_search || recommendations.enabled()));
    let app = App {
        db,
        secret: secret.into(),
        search: search::Search::new(hybrid_search, embedder.clone())?,
        recommendations,
        tax: setting("TAX_BASIS_POINTS", "0")
            .parse::<i64>()?
            .clamp(0, 10000),
        payment_hold_hours: setting("PAYMENT_HOLD_HOURS", "24").parse()?,
        allow_test_accounts: setting("ALLOW_TEST_ACCOUNTS", "false") == "true",
        app_url: Arc::new(storefront_url(&setting(
            "APP_URL",
            "http://127.0.0.1:5173",
        ))?),
    };
    anyhow::ensure!(
        (1..=720).contains(&app.payment_hold_hours),
        "PAYMENT_HOLD_HOURS must be between 1 and 720"
    );
    if seed_test {
        return test_seed::run(&app).await;
    }
    auth::seed(&app).await?;
    if setting("DEMO_CATALOG", "false") == "true" {
        catalog::seed(&app).await?;
    }
    search::prepare(&app).await?;
    tokio::fs::create_dir_all(uploads::PUBLIC_DIR).await?;
    tokio::fs::create_dir_all(uploads::PRIVATE_DIR).await?;
    tokio::spawn(search::index_worker(app.clone()));
    tokio::spawn(commerce::expire_unpaid_checkouts(app.clone()));
    tokio::spawn(auth::prune_expired_challenges(app.db.clone()));
    if app.recommendations.enabled() {
        tokio::spawn(recommendations::refresh_worker(app.clone()));
    }
    if embedder.enabled() {
        tokio::spawn(embeddings::worker(
            app.db.clone(),
            embedder,
            app.recommendations.enabled(),
        ));
    }
    let compress_json = SizeAbove::default().and(|_, _, headers: &HeaderMap, _: &Extensions| {
        headers.get(CONTENT_TYPE).is_some_and(|content_type| {
            content_type
                .as_bytes()
                .eq_ignore_ascii_case(b"application/json")
        })
    });
    let router = Router::new()
        .nest(
            "/api",
            auth::routes()
                .merge(catalog::routes())
                .merge(commerce::routes())
                .merge(recommendations::routes())
                .merge(operations::routes())
                .fallback(|| async { Error::missing() })
                .layer(CompressionLayer::new().compress_when(compress_json))
                .layer(middleware::from_fn(require_acceptable_encoding))
                .layer(middleware::from_fn_with_state(
                    app.clone(),
                    require_same_origin_write,
                ))
                .layer(middleware::from_fn(prevent_api_caching)),
        )
        .nest_service("/uploads", uploads::public_files(uploads::PUBLIC_DIR))
        .fallback_service(
            ServeDir::new("../frontend/dist")
                .fallback(ServeFile::new("../frontend/dist/index.html")),
        )
        .layer(middleware::from_fn(browser_security_headers))
        .layer(
            tower_http::trace::TraceLayer::new_for_http().make_span_with(|request: &Request| {
                tracing::debug_span!(
                    "request",
                    method = %request.method(),
                    route = request.extensions().get::<MatchedPath>().map(MatchedPath::as_str),
                )
            }),
        )
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(setting("BIND", "127.0.0.1:8080")).await?;
    tracing::info!(address = %listener.local_addr()?, "Tosca Market ready");
    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_database_datetime, require_acceptable_encoding};
    use axum::{
        Router,
        body::Body,
        http::{
            Request, StatusCode,
            header::{ACCEPT_ENCODING, VARY},
        },
        middleware,
        routing::post,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use time::macros::datetime;
    use tower::ServiceExt;
    use tower_http::compression::CompressionLayer;

    #[test]
    fn preserves_database_datetime_json_precision() {
        for (timestamp, expected) in [
            (datetime!(2026-11-21 08:09:10), "2026-11-21T08:09:10"),
            (datetime!(2026-11-21 08:09:10.1), "2026-11-21T08:09:10.100"),
            (
                datetime!(2026-11-21 08:09:10.1234),
                "2026-11-21T08:09:10.123400",
            ),
        ] {
            assert_eq!(format_database_datetime(timestamp), expected);
        }
    }
    #[tokio::test]
    async fn rejects_unacceptable_encodings_before_running_a_handler() {
        let calls = Arc::new(AtomicUsize::new(0));
        let handler_calls = Arc::clone(&calls);
        let router = Router::new()
            .route(
                "/",
                post(move || async move {
                    handler_calls.fetch_add(1, Ordering::Relaxed);
                    StatusCode::CREATED
                }),
            )
            .layer(CompressionLayer::new())
            .layer(middleware::from_fn(require_acceptable_encoding));

        for encoding in ["identity;q=0, gzip;q=0", "*;q=0"] {
            let request = Request::post("/")
                .header(ACCEPT_ENCODING, encoding)
                .body(Body::empty())
                .expect("valid test request");
            let Ok(response) = router.clone().oneshot(request).await;
            assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE, "{encoding}");
            assert_eq!(response.headers()[VARY], "accept-encoding");
        }
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let request = Request::post("/")
            .header(ACCEPT_ENCODING, "identity;q=0, zstd")
            .body(Body::empty())
            .expect("valid test request");
        let Ok(response) = router.oneshot(request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}
