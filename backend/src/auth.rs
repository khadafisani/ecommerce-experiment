use super::*;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{ConnectInfo, DefaultBodyLimit, FromRequestParts, State},
    http::request::Parts,
    routing::{get, post},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{net::SocketAddr, time::Duration};
use time::{OffsetDateTime, PlainDateTime};
use tokio::sync::Semaphore;
use uuid::Uuid;

static PASSWORD_JOBS: Semaphore = Semaphore::const_new(4);
static RECOVERY_MAIL_JOBS: Semaphore = Semaphore::const_new(4);
const DUMMY_PASSWORD: &str = "$argon2id$v=19$m=19456,t=2,p=1$MDAwMDAwMDAwMDAwMDAwMA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const SESSION_AGE: i64 = 30 * 24 * 60 * 60;
const ACCESS_AGE: i64 = 15 * 60;
const ACCESS_KIND: &str = "cookie_access";
const REFRESH_KIND: &str = "cookie_refresh";
const MARKER: &str = "tosca_session";

pub fn routes() -> Router<App> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/guest", post(guest))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/session-upgrade", post(upgrade_guest_session))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/forgot", post(forgot))
        .route("/auth/reset", post(reset))
        .route("/shops", post(open_shop))
        .route("/notifications", get(notifications))
        .route("/notifications/read", post(read_notifications))
        .layer(DefaultBodyLimit::max(16 * 1024))
}

#[derive(Clone, sqlx::FromRow, Serialize)]
pub struct User {
    pub id: i64,
    name: String,
    email: String,
    admin: bool,
    guest: bool,
}
impl User {
    pub fn is_guest(&self) -> bool {
        self.guest
    }

    pub async fn optional(headers: &HeaderMap, app: &App) -> Result<Option<Self>> {
        if cookie(headers, session_cookies(app)[0].0).is_some() || cookie(headers, MARKER).is_some()
        {
            Self::authenticate(headers, app).await.map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn admin(&self) -> Result<()> {
        if self.admin {
            Ok(())
        } else {
            Err(Error::forbidden())
        }
    }
    async fn authenticate(headers: &HeaderMap, app: &App) -> Result<Self> {
        let token = cookie(headers, session_cookies(app)[0].0).ok_or_else(Error::unauthorized)?;
        let claims = decode(app, token, ACCESS_KIND)?;
        sqlx::query_as("SELECT u.id,u.name,u.email,u.admin,u.guest FROM users u JOIN sessions s ON s.user_id=u.id WHERE u.id=? AND s.id=? AND NOT s.revoked AND NOT u.suspended AND s.expires_at>UTC_TIMESTAMP() AND (? OR u.email NOT LIKE '%@seed.tosca.invalid')")
            .bind(claims.sub).bind(claims.sid).bind(app.allow_test_accounts).fetch_optional(&app.db).await?.ok_or_else(Error::unauthorized)
    }
}
impl FromRequestParts<App> for User {
    type Rejection = Error;
    async fn from_request_parts(parts: &mut Parts, app: &App) -> Result<Self> {
        Self::authenticate(&parts.headers, app).await
    }
}
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: i64,
    sid: String,
    exp: i64,
    kind: String,
    jti: String,
}
fn decode(app: &App, token: &str, kind: &str) -> Result<Claims> {
    let claims = jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(app.secret.as_bytes()).map_err(Error::internal)?,
        &Validation::default(),
    )
    .map_err(|_| Error::unauthorized())?
    .claims;
    if claims.kind != kind {
        return Err(Error::unauthorized());
    }
    Ok(claims)
}
fn token(app: &App, user: i64, sid: &str, kind: &str, duration: i64) -> Result<String> {
    jsonwebtoken::encode(
        &Header::default(),
        &Claims {
            sub: user,
            sid: sid.into(),
            kind: kind.into(),
            exp: OffsetDateTime::now_utc().unix_timestamp() + duration,
            jti: Uuid::new_v4().to_string(),
        },
        &EncodingKey::from_secret(app.secret.as_bytes()).map_err(Error::internal)?,
    )
    .map_err(Error::internal)
}
pub fn digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub async fn hash(password: String) -> Result<String> {
    if !(10..=128).contains(&password.len()) {
        return Err(Error::bad("Password must be 10 to 128 bytes"));
    }
    let permit = PASSWORD_JOBS.try_acquire().map_err(|_| Error::busy())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let salt = Uuid::new_v4();
        argon2::PasswordHasher::hash_password_with_salt(
            &Argon2::default(),
            password.as_bytes(),
            salt.as_bytes(),
        )
        .map(|hash| hash.to_string())
        .map_err(Error::internal)
    })
    .await
    .map_err(Error::internal)?
}
async fn verify(password: String, encoded: String) -> Result<Option<String>> {
    let permit = PASSWORD_JOBS.try_acquire().map_err(|_| Error::busy())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let valid = PasswordHash::new(&encoded).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        });
        valid.then_some(encoded)
    })
    .await
    .map_err(Error::internal)
}
pub fn email(value: &str) -> Result<String> {
    let value = text(value, 254)?.to_lowercase();
    let _: lettre::Address = value
        .parse()
        .map_err(|_| Error::bad("Enter a valid email address"))?;
    Ok(value)
}
pub async fn seed(app: &App) -> anyhow::Result<()> {
    let (mail, password) = (setting("ADMIN_EMAIL", ""), setting("ADMIN_PASSWORD", ""));
    if mail.is_empty() && password.is_empty() {
        return Ok(());
    }
    if mail.is_empty() || password.is_empty() {
        anyhow::bail!("Set both ADMIN_EMAIL and ADMIN_PASSWORD to seed an administrator");
    }
    let mail = email(&mail).map_err(|e| anyhow::anyhow!(e.1))?;
    let exists: Option<bool> = sqlx::query_scalar("SELECT admin FROM users WHERE email=?")
        .bind(&mail)
        .fetch_optional(&app.db)
        .await?;
    match exists {
        Some(true) => return Ok(()),
        Some(false) => anyhow::bail!("ADMIN_EMAIL already belongs to a non-admin account"),
        None => {}
    }
    let password = hash(password).await.map_err(|e| anyhow::anyhow!(e.1))?;
    let mut tx = app.db.begin().await?;
    let id = sqlx::query(
        "INSERT INTO users(email,name,password,admin) VALUES (?,'Administrator','!',TRUE)",
    )
    .bind(mail)
    .execute(&mut *tx)
    .await?
    .last_insert_id();
    save_password(&mut tx, i64::try_from(id)?, &password)
        .await
        .map_err(|error| anyhow::anyhow!(error.1))?;
    tx.commit().await?;
    Ok(())
}

#[derive(Deserialize)]
struct Register {
    name: String,
    email: String,
    password: String,
    shop: Option<String>,
}
pub async fn create_shop(tx: &mut Transaction<'_, MySql>, user: i64, name: &str) -> Result<i64> {
    let id = inserted(
        sqlx::query("INSERT INTO shops(name,owner_id) VALUES (?,?)")
            .bind(text(name, 120)?)
            .bind(user)
            .execute(&mut **tx)
            .await?,
    )?;
    sqlx::query("INSERT INTO members(shop_id,user_id) VALUES (?,?)")
        .bind(id)
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}
async fn guest(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
) -> Result<Response> {
    rate_limit(&app.db, "guest", peer.ip(), 10).await?;
    let identity = format!("{}@guest.tosca.invalid", Uuid::new_v4());
    let mut tx = app.db.begin().await?;
    let id = inserted(
        sqlx::query("INSERT INTO users(name,email,password,guest) VALUES ('Guest',?,'!',TRUE)")
            .bind(identity)
            .execute(&mut *tx)
            .await?,
    )?;
    let response = issue(&app, &mut tx, id, true).await?;
    tx.commit().await?;
    Ok(response)
}
async fn register(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(input): Json<Register>,
) -> Result<Response> {
    rate_limit(&app.db, "register", peer.ip(), 5).await?;
    let name = text(&input.name, 120)?;
    let email = email(&input.email)?;
    if let Some(shop) = &input.shop {
        text(shop, 120)?;
    }
    let password = hash(input.password).await?;
    let mut tx = app.db.begin().await?;
    let id = inserted(
        sqlx::query("INSERT INTO users(name,email,password) VALUES (?,?,'!')")
            .bind(name)
            .bind(email)
            .execute(&mut *tx)
            .await?,
    )?;
    save_password(&mut tx, id, &password).await?;
    if let Some(shop) = input.shop {
        create_shop(&mut tx, id, &shop).await?;
    }
    let response = issue(&app, &mut tx, id, false).await?;
    tx.commit().await?;
    Ok(response)
}
#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
    portal: Option<String>,
}
async fn login(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(input): Json<Login>,
) -> Result<Response> {
    rate_limit(&app.db, "login", peer.ip(), 5).await?;
    if input.password.len() > 128 {
        return Err(Error::unauthorized());
    }
    let account: Option<(i64, String)> =
        sqlx::query_as("SELECT u.id,p.password_hash FROM users u JOIN user_credentials p ON p.user_id=u.id WHERE u.email=? AND NOT u.suspended AND (? OR u.email NOT LIKE '%@seed.tosca.invalid')")
            .bind(email(&input.email)?)
            .bind(app.allow_test_accounts)
            .fetch_optional(&app.db)
            .await?;
    let (id, password) = account.unwrap_or_else(|| (0, DUMMY_PASSWORD.to_owned()));
    let verified = verify(input.password, password)
        .await?
        .ok_or_else(Error::unauthorized)?;
    let mut tx = app.db.begin().await?;
    let admin = sqlx::query_scalar::<_, bool>(
        "SELECT admin FROM users WHERE id=? AND NOT suspended FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(Error::unauthorized)?;
    let current: String =
        sqlx::query_scalar("SELECT password_hash FROM user_credentials WHERE user_id=? FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if current != verified {
        return Err(Error::unauthorized());
    }
    match input.portal.as_deref() {
        Some("admin") if !admin => return Err(Error::forbidden()),
        Some("merchant") => {
            let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM members m JOIN shops s ON s.id=m.shop_id WHERE m.user_id=? AND m.active AND NOT s.suspended)").bind(id).fetch_one(&mut *tx).await?;
            if !allowed {
                return Err(Error::forbidden());
            }
        }
        _ => {}
    }
    let response = issue(&app, &mut tx, id, false).await?;
    tx.commit().await?;
    Ok(response)
}

pub(super) async fn rate_limit(
    db: &MySqlPool,
    scope: &str,
    identity: impl std::fmt::Display,
    max: i32,
) -> Result<()> {
    let window = OffsetDateTime::now_utc().unix_timestamp() / 60;
    let identity = digest(&format!("{scope}:{identity}"));
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO login_limits(identity_hash,window_start,attempts) VALUES (?,?,1) ON DUPLICATE KEY UPDATE attempts=LEAST(attempts+1,?)")
        .bind(&identity).bind(window).bind(max+1).execute(&mut *tx).await?;
    let attempts: i32 = sqlx::query_scalar(
        "SELECT attempts FROM login_limits WHERE identity_hash=? AND window_start=?",
    )
    .bind(identity)
    .bind(window)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if attempts > max {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again in a minute".into(),
        ));
    }
    Ok(())
}

pub(super) async fn prune_expired_challenges(db: MySqlPool) {
    loop {
        let cutoff = OffsetDateTime::now_utc().unix_timestamp() / 60 - 60;
        let result = tokio::try_join!(
            sqlx::query("DELETE FROM login_limits WHERE window_start<? ORDER BY window_start LIMIT 1000")
                .bind(cutoff).execute(&db),
            sqlx::query("DELETE FROM password_resets WHERE expires_at<=UTC_TIMESTAMP() ORDER BY expires_at LIMIT 1000")
                .execute(&db),
        );
        let delay = match result {
            Ok((limits, resets))
                if limits.rows_affected() == 1000 || resets.rows_affected() == 1000 =>
            {
                2
            }
            Ok(_) => 300,
            Err(error) => {
                tracing::warn!(%error, "authentication cleanup failed");
                60
            }
        };
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}
async fn issue(
    app: &App,
    tx: &mut Transaction<'_, MySql>,
    id: i64,
    guest: bool,
) -> Result<Response> {
    let sid = Uuid::new_v4().to_string();
    let refresh = token(app, id, &sid, REFRESH_KIND, SESSION_AGE)?;
    sqlx::query("INSERT INTO sessions(id,user_id,refresh_hash,expires_at) VALUES (?,?,?,UTC_TIMESTAMP()+INTERVAL 30 DAY)")
        .bind(&sid).bind(id).bind(digest(&refresh)).execute(&mut **tx).await?;
    tokens(app, id, &sid, &refresh, guest, SESSION_AGE)
}

fn session_cookies(app: &App) -> [(&'static str, &'static str); 2] {
    if app.app_url.scheme() == "https" {
        // Host-prefixed cookies prevent a sibling subdomain setting cookies for this origin.
        [("__Host-tosca_access", "/"), ("__Host-tosca_refresh", "/")]
    } else {
        [("tosca_access", "/api"), ("tosca_refresh", "/api/auth")]
    }
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let mut matches = headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .filter_map(|(key, value)| (key == name && !value.is_empty()).then_some(value));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn set_cookie(
    app: &App,
    response: &mut Response,
    (name, path): (&str, &str),
    value: &str,
    http_only: bool,
    max_age: Option<i64>,
) -> Result<()> {
    let mut header = format!("{name}={value}; Path={path}; SameSite=Strict");
    if http_only {
        header.push_str("; HttpOnly");
    }
    if app.app_url.scheme() == "https" {
        header.push_str("; Secure");
    }
    if let Some(max_age) = max_age {
        use std::fmt::Write as _;
        write!(header, "; Max-Age={max_age}").map_err(Error::internal)?;
    }
    response.headers_mut().append(
        http::header::SET_COOKIE,
        http::HeaderValue::from_str(&header).map_err(Error::internal)?,
    );
    Ok(())
}

fn tokens(
    app: &App,
    id: i64,
    sid: &str,
    refresh: &str,
    guest: bool,
    remaining: i64,
) -> Result<Response> {
    let mut response = ok().into_response();
    let [access_cookie, refresh_cookie] = session_cookies(app);
    let access_age = ACCESS_AGE.min(remaining);
    let access = token(app, id, sid, ACCESS_KIND, access_age)?;
    let kind = if guest { "guest" } else { "account" };
    let marker = format!("{kind}.{sid}.{}", Uuid::new_v4());
    set_cookie(
        app,
        &mut response,
        access_cookie,
        &access,
        true,
        guest.then_some(access_age),
    )?;
    set_cookie(
        app,
        &mut response,
        refresh_cookie,
        refresh,
        true,
        guest.then_some(remaining),
    )?;
    set_cookie(
        app,
        &mut response,
        (MARKER, "/"),
        &marker,
        false,
        guest.then_some(remaining),
    )?;
    Ok(response)
}

fn clear_cookies(app: &App, mut response: Response) -> Result<Response> {
    for location in session_cookies(app) {
        set_cookie(app, &mut response, location, "", true, Some(0))?;
    }
    set_cookie(app, &mut response, (MARKER, "/"), "", false, Some(0))?;
    Ok(response)
}

#[derive(Deserialize)]
struct LegacyRefresh {
    refresh: String,
}

async fn upgrade_guest_session(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(input): Json<LegacyRefresh>,
) -> Result<Response> {
    rate_limit(&app.db, "guest-upgrade", peer.ip(), 10).await?;
    if cookie(&headers, session_cookies(&app)[1].0).is_some() {
        return Err(Error::conflict("A browser session is already active"));
    }
    rotate(&app, &input.refresh, true).await
}

async fn refresh(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    let result = match cookie(&headers, session_cookies(&app)[1].0) {
        Some(token) => rotate(&app, token, false).await,
        None => Err(Error::unauthorized()),
    };
    match result {
        Err(error) if error.0 == StatusCode::UNAUTHORIZED => {
            clear_cookies(&app, error.into_response())
        }
        result => result,
    }
}

async fn rotate(app: &App, input: &str, legacy_guest: bool) -> Result<Response> {
    let kind = if legacy_guest {
        "refresh"
    } else {
        REFRESH_KIND
    };
    let claims = decode(app, input, kind)?;
    let mut tx = app.db.begin().await?;
    let guest: bool = sqlx::query_scalar("SELECT guest FROM users WHERE id=? AND NOT suspended AND (? OR email NOT LIKE '%@seed.tosca.invalid') FOR UPDATE")
        .bind(claims.sub)
        .bind(app.allow_test_accounts)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(Error::unauthorized)?;
    if legacy_guest && !guest {
        return Err(Error::unauthorized());
    }
    let stored: Option<String> = sqlx::query_scalar("SELECT refresh_hash FROM sessions WHERE id=? AND user_id=? AND NOT revoked AND expires_at>UTC_TIMESTAMP() FOR UPDATE")
        .bind(&claims.sid).bind(claims.sub).fetch_optional(&mut *tx).await?;
    let stored = stored.ok_or_else(Error::unauthorized)?;
    if stored != digest(input) {
        // A completed legacy upgrade must not be revoked by another tab with an old token.
        if legacy_guest {
            return Err(Error::unauthorized());
        }
        sqlx::query("UPDATE sessions SET revoked=TRUE WHERE id=?")
            .bind(&claims.sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(Error::unauthorized());
    }
    let duration = claims.exp - OffsetDateTime::now_utc().unix_timestamp();
    if duration <= 0 {
        return Err(Error::unauthorized());
    }
    let refresh = token(app, claims.sub, &claims.sid, REFRESH_KIND, duration)?;
    sqlx::query("UPDATE sessions SET refresh_hash=? WHERE id=?")
        .bind(digest(&refresh))
        .bind(&claims.sid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tokens(app, claims.sub, &claims.sid, &refresh, guest, duration)
}
async fn logout(State(app): State<App>, headers: HeaderMap) -> Result<Response> {
    if let Some(refresh) = cookie(&headers, session_cookies(&app)[1].0)
        && let Ok(claims) = decode(&app, refresh, REFRESH_KIND)
    {
        sqlx::query("UPDATE sessions SET revoked=TRUE WHERE id=? AND user_id=?")
            .bind(claims.sid)
            .bind(claims.sub)
            .execute(&app.db)
            .await?;
    }
    clear_cookies(&app, ok().into_response())
}
async fn me(State(app): State<App>, user: User) -> Result {
    let shops: Vec<(i64,String,bool)> = sqlx::query_as("SELECT s.id,s.name,s.owner_id=m.user_id FROM members m JOIN shops s ON s.id=m.shop_id WHERE m.user_id=? AND m.active AND NOT s.suspended ORDER BY s.id").bind(user.id).fetch_all(&app.db).await?;
    Ok(Json(
        json!({"user":user,"shops":shops.into_iter().map(|(id,name,owner)|json!({"id":id,"name":name,"owner":owner})).collect::<Vec<_>>()}),
    ))
}
#[derive(Deserialize, Serialize)]
struct ShopInput {
    name: String,
    key: String,
}
async fn open_shop(State(app): State<App>, user: User, Json(input): Json<ShopInput>) -> Result {
    if user.guest {
        return Err(Error::forbidden());
    }
    let mut tx = app.db.begin().await?;
    let id = if let Some(id) = claim(&mut tx, user.id, "shop", &input.key, &input).await? {
        id
    } else {
        let id = create_shop(&mut tx, user.id, &input.name).await?;
        finish(&mut tx, user.id, "shop", &input.key, id).await?;
        id
    };
    tx.commit().await?;
    Ok(Json(json!({"id":id})))
}
#[derive(Deserialize)]
struct Forgot {
    email: String,
}
async fn forgot(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(input): Json<Forgot>,
) -> Result {
    rate_limit(&app.db, "forgot", peer.ip(), 5).await?;
    let email = email(&input.email)?;
    let url = setting("SMTP_URL", "");
    let from = setting("SMTP_FROM", "");
    if url.is_empty() || from.is_empty() {
        return Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Password recovery email is unavailable".into(),
        ));
    }
    let permit = RECOVERY_MAIL_JOBS
        .try_acquire()
        .map_err(|_| Error::busy())?;
    let mut tx = app.db.begin().await?;
    let id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE email=? AND NOT suspended AND NOT guest AND EXISTS(SELECT 1 FROM user_credentials WHERE user_id=users.id) FOR UPDATE")
            .bind(&email)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(id) = id {
        let permitted: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM password_resets WHERE user_id=? AND created_at>UTC_TIMESTAMP()-INTERVAL 1 MINUTE)").bind(id).fetch_one(&mut *tx).await?;
        if permitted {
            let reset = Uuid::new_v4().to_string();
            let link = format!(
                "{}/reset#token={reset}",
                app.app_url.origin().ascii_serialization()
            );
            let mail = Message::builder()
                .from(from.parse().map_err(Error::internal)?)
                .to(email.parse().map_err(Error::internal)?)
                .subject("Reset your Tosca Market password")
                .body(format!("Reset your password within 30 minutes: {link}"))
                .map_err(Error::internal)?;
            let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(&url)
                .map_err(Error::internal)?
                .timeout(Some(Duration::from_secs(15)))
                .build();
            sqlx::query("INSERT INTO password_resets(token_hash,user_id,expires_at) VALUES (?,?,UTC_TIMESTAMP()+INTERVAL 30 MINUTE)").bind(digest(&reset)).bind(id).execute(&mut *tx).await?;
            tx.commit().await?;
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = transport.send(mail).await {
                    tracing::error!(%error, "password recovery mail failed");
                }
            });
            return Ok(ok());
        }
    }
    Ok(ok())
}
#[derive(Deserialize)]
struct Reset {
    token: String,
    password: String,
}
async fn reset(
    State(app): State<App>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(input): Json<Reset>,
) -> Result {
    rate_limit(&app.db, "reset", peer.ip(), 5).await?;
    let expired = || Error::bad("Reset link expired");
    let token_hash = digest(&input.token);
    let id: i64 = sqlx::query_scalar(
        "SELECT user_id FROM password_resets WHERE token_hash=? AND expires_at>UTC_TIMESTAMP()",
    )
    .bind(&token_hash)
    .fetch_optional(&app.db)
    .await?
    .ok_or_else(expired)?;
    let password = hash(input.password).await?;
    let mut tx = app.db.begin().await?;
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM users WHERE id=? AND NOT suspended AND NOT guest FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(expired)?;
    sqlx::query_scalar::<_, i64>("SELECT user_id FROM password_resets WHERE token_hash=? AND user_id=? AND expires_at>UTC_TIMESTAMP() FOR UPDATE")
        .bind(token_hash).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(expired)?;
    change_password(&mut tx, id, &password).await?;
    tx.commit().await?;
    Ok(ok())
}
pub async fn change_password(
    tx: &mut Transaction<'_, MySql>,
    id: i64,
    password: &str,
) -> Result<()> {
    sqlx::query_scalar::<_, i64>("SELECT id FROM users WHERE id=? FOR UPDATE")
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
    save_password(tx, id, password).await?;
    sqlx::query("UPDATE sessions SET revoked=TRUE WHERE user_id=?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM password_resets WHERE user_id=?")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(super) async fn save_password(
    tx: &mut Transaction<'_, MySql>,
    id: i64,
    password: &str,
) -> Result<()> {
    sqlx::query("INSERT INTO user_credentials(user_id,password_hash) VALUES (?,?) ON DUPLICATE KEY UPDATE password_hash=VALUES(password_hash)")
        .bind(id).bind(password).execute(&mut **tx).await?;
    Ok(())
}
async fn notifications(State(app): State<App>, user: User) -> Result {
    let rows: Vec<(i64, String, PlainDateTime, Option<PlainDateTime>)> = sqlx::query_as(
        "SELECT id,message,created_at,read_at FROM notifications WHERE user_id=? ORDER BY id DESC LIMIT 100",
    )
    .bind(user.id)
    .fetch_all(&app.db)
    .await?;
    let notifications: Vec<_> = rows
        .into_iter()
        .map(|(id, message, created_at, read_at)| {
            json!({
                "id": id,
                "message": message,
                "created_at": format_database_datetime(created_at),
                "read_at": read_at.map(format_database_datetime),
            })
        })
        .collect();
    Ok(Json(json!(notifications)))
}
async fn read_notifications(State(app): State<App>, user: User) -> Result {
    sqlx::query(
        "UPDATE notifications SET read_at=UTC_TIMESTAMP() WHERE user_id=? AND read_at IS NULL",
    )
    .bind(user.id)
    .execute(&app.db)
    .await?;
    Ok(ok())
}
