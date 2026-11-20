use super::*;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{ConnectInfo, FromRequestParts, State},
    http::request::Parts,
    routing::{get, post},
};
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use uuid::Uuid;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/guest", post(guest))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/forgot", post(forgot))
        .route("/auth/reset", post(reset))
        .route("/shops", post(open_shop))
        .route("/notifications", get(notifications))
        .route("/notifications/read", post(read_notifications))
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
        if headers.contains_key(http::header::AUTHORIZATION) {
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
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(Error::unauthorized)?;
        let claims = decode(app, token, "access")?;
        sqlx::query_as("SELECT u.id,u.name,u.email,u.admin,u.guest FROM users u JOIN sessions s ON s.user_id=u.id WHERE u.id=? AND s.id=? AND NOT s.revoked AND NOT u.suspended AND s.expires_at>UTC_TIMESTAMP()")
            .bind(claims.sub).bind(claims.sid).fetch_optional(&app.db).await?.ok_or_else(Error::unauthorized)
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
            exp: Utc::now().timestamp() + duration,
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
    tokio::task::spawn_blocking(move || {
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
async fn verify(password: String, encoded: String) -> Result<bool> {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&encoded).is_ok_and(|hash| {
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        })
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
    sqlx::query("INSERT INTO users(email,name,password,admin) VALUES (?,'Administrator',?,TRUE)")
        .bind(mail)
        .bind(password)
        .execute(&app.db)
        .await?;
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
async fn guest(State(app): State<App>) -> Result {
    let identity = format!("{}@guest.tosca.invalid", Uuid::new_v4());
    let id = inserted(
        sqlx::query("INSERT INTO users(name,email,password,guest) VALUES ('Guest',?,'!',TRUE)")
            .bind(identity)
            .execute(&app.db)
            .await?,
    )?;
    issue(&app, id).await
}
async fn register(State(app): State<App>, Json(input): Json<Register>) -> Result {
    let password = hash(input.password).await?;
    let mut tx = app.db.begin().await?;
    let id = inserted(
        sqlx::query("INSERT INTO users(name,email,password) VALUES (?,?,?)")
            .bind(text(&input.name, 120)?)
            .bind(email(&input.email)?)
            .bind(password)
            .execute(&mut *tx)
            .await?,
    )?;
    if let Some(shop) = input.shop {
        create_shop(&mut tx, id, &shop).await?;
    }
    tx.commit().await?;
    issue(&app, id).await
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
) -> Result {
    let window = Utc::now().timestamp() / 60;
    let identity = digest(&peer.ip().to_string());
    let mut tx = app.db.begin().await?;
    sqlx::query("INSERT INTO login_limits(identity_hash,window_start,attempts) VALUES (?,?,1) ON DUPLICATE KEY UPDATE attempts=LEAST(attempts+1,6)")
        .bind(&identity).bind(window).execute(&mut *tx).await?;
    let attempts: i32 = sqlx::query_scalar(
        "SELECT attempts FROM login_limits WHERE identity_hash=? AND window_start=?",
    )
    .bind(&identity)
    .bind(window)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    if attempts > 5 {
        return Err(Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Try again in a minute".into(),
        ));
    }
    if input.password.len() > 128 {
        return Err(Error::unauthorized());
    }
    let account: Option<(i64, String, bool)> =
        sqlx::query_as("SELECT id,password,admin FROM users WHERE email=? AND NOT suspended")
            .bind(email(&input.email)?)
            .fetch_optional(&app.db)
            .await?;
    let (id, password, admin) = account.ok_or_else(Error::unauthorized)?;
    if !verify(input.password, password).await? {
        return Err(Error::unauthorized());
    }
    match input.portal.as_deref() {
        Some("admin") if !admin => return Err(Error::forbidden()),
        Some("merchant") => {
            let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM members m JOIN shops s ON s.id=m.shop_id WHERE m.user_id=? AND m.active AND NOT s.suspended)").bind(id).fetch_one(&app.db).await?;
            if !allowed {
                return Err(Error::forbidden());
            }
        }
        _ => {}
    }
    issue(&app, id).await
}
async fn issue(app: &App, id: i64) -> Result {
    let sid = Uuid::new_v4().to_string();
    let refresh = token(app, id, &sid, "refresh", 2_592_000)?;
    sqlx::query("INSERT INTO sessions(id,user_id,refresh_hash,expires_at) VALUES (?,?,?,UTC_TIMESTAMP()+INTERVAL 30 DAY)")
        .bind(&sid).bind(id).bind(digest(&refresh)).execute(&app.db).await?;
    tokens(app, id, &sid, refresh)
}
fn tokens(app: &App, id: i64, sid: &str, refresh: String) -> Result {
    Ok(Json(
        json!({"access":token(app,id,sid,"access",900)?, "refresh":refresh}),
    ))
}
#[derive(Deserialize)]
struct Refresh {
    refresh: String,
}
async fn refresh(State(app): State<App>, Json(input): Json<Refresh>) -> Result {
    let claims = decode(&app, &input.refresh, "refresh")?;
    let mut tx = app.db.begin().await?;
    let stored: Option<String> = sqlx::query_scalar("SELECT s.refresh_hash FROM sessions s JOIN users u ON u.id=s.user_id WHERE s.id=? AND s.user_id=? AND NOT s.revoked AND NOT u.suspended AND s.expires_at>UTC_TIMESTAMP() FOR UPDATE")
        .bind(&claims.sid).bind(claims.sub).fetch_optional(&mut *tx).await?;
    let stored = stored.ok_or_else(Error::unauthorized)?;
    if stored != digest(&input.refresh) {
        sqlx::query("UPDATE sessions SET revoked=TRUE WHERE id=?")
            .bind(&claims.sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(Error::unauthorized());
    }
    let duration = claims.exp - Utc::now().timestamp();
    if duration <= 0 {
        return Err(Error::unauthorized());
    }
    let refresh = token(&app, claims.sub, &claims.sid, "refresh", duration)?;
    sqlx::query("UPDATE sessions SET refresh_hash=? WHERE id=?")
        .bind(digest(&refresh))
        .bind(&claims.sid)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    tokens(&app, claims.sub, &claims.sid, refresh)
}
async fn logout(State(app): State<App>, Json(input): Json<Refresh>) -> Result {
    let claims = decode(&app, &input.refresh, "refresh")?;
    sqlx::query("UPDATE sessions SET revoked=TRUE WHERE id=? AND refresh_hash=?")
        .bind(claims.sid)
        .bind(digest(&input.refresh))
        .execute(&app.db)
        .await?;
    Ok(ok())
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
async fn forgot(State(app): State<App>, Json(input): Json<Forgot>) -> Result {
    let email = email(&input.email)?;
    let url = setting("SMTP_URL", "");
    let from = setting("SMTP_FROM", "");
    if url.is_empty() || from.is_empty() {
        return Err(Error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Password recovery email is unavailable".into(),
        ));
    }
    let id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE email=? AND NOT suspended")
            .bind(&email)
            .fetch_optional(&app.db)
            .await?;
    if let Some(id) = id {
        let permitted: bool = sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM password_resets WHERE user_id=? AND created_at>UTC_TIMESTAMP()-INTERVAL 1 MINUTE)").bind(id).fetch_one(&app.db).await?;
        if permitted {
            let reset = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO password_resets(token_hash,user_id,expires_at) VALUES (?,?,UTC_TIMESTAMP()+INTERVAL 30 MINUTE)").bind(digest(&reset)).bind(id).execute(&app.db).await?;
            let link = format!(
                "{}/reset?token={reset}",
                setting("APP_URL", "http://127.0.0.1:5173").trim_end_matches('/')
            );
            let mail = Message::builder()
                .from(from.parse().map_err(Error::internal)?)
                .to(email.parse().map_err(Error::internal)?)
                .subject("Reset your Tosca Market password")
                .body(format!("Reset your password within 30 minutes: {link}"))
                .map_err(Error::internal)?;
            let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(&url)
                .map_err(Error::internal)?
                .build();
            tokio::spawn(async move {
                if let Err(error) = transport.send(mail).await {
                    tracing::error!(%error, "password recovery mail failed");
                }
            });
        }
    }
    Ok(ok())
}
#[derive(Deserialize)]
struct Reset {
    token: String,
    password: String,
}
async fn reset(State(app): State<App>, Json(input): Json<Reset>) -> Result {
    let password = hash(input.password).await?;
    let mut tx = app.db.begin().await?;
    let id: i64 = sqlx::query_scalar("SELECT user_id FROM password_resets WHERE token_hash=? AND expires_at>UTC_TIMESTAMP() FOR UPDATE").bind(digest(&input.token)).fetch_optional(&mut *tx).await?.ok_or_else(|| Error::bad("Reset link expired"))?;
    change_password(&mut tx, id, &password).await?;
    tx.commit().await?;
    Ok(ok())
}
pub async fn change_password(
    tx: &mut Transaction<'_, MySql>,
    id: i64,
    password: &str,
) -> Result<()> {
    sqlx::query("UPDATE users SET password=? WHERE id=?")
        .bind(password)
        .bind(id)
        .execute(&mut **tx)
        .await?;
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
async fn notifications(State(app): State<App>, user: User) -> Result {
    let rows: Vec<(i64,String,chrono::NaiveDateTime,Option<chrono::NaiveDateTime>)> = sqlx::query_as("SELECT id,message,created_at,read_at FROM notifications WHERE user_id=? ORDER BY id DESC LIMIT 100").bind(user.id).fetch_all(&app.db).await?;
    Ok(Json(json!(rows.into_iter().map(|(id,message,created_at,read_at)|json!({"id":id,"message":message,"created_at":created_at,"read_at":read_at})).collect::<Vec<_>>())))
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
