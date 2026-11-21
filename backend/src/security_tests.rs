use super::*;
use axum::{body::to_bytes, extract::ConnectInfo};
use sqlx::{AssertSqlSafe, Connection, MySqlConnection};
use std::net::{Ipv4Addr, SocketAddr};

#[tokio::test]
#[ignore = "requires local MariaDB with permission to create a disposable database"]
async fn protects_credentials_and_expires_reservations() -> anyhow::Result<()> {
    let options: MySqlConnectOptions = dotenvy::var("DATABASE_URL")?.parse()?;
    anyhow::ensure!(matches!(
        options.get_host(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]"
    ));
    let name = format!("tosca_audit_{}", uuid::Uuid::new_v4().simple());
    let mut control = MySqlConnection::connect_with(&options.clone().database("mysql")).await?;
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE `{name}`")))
        .execute(&mut control)
        .await?;
    let checked = async {
        let db = MySqlPool::connect_with(options.database(&name)).await?;
        let result = check_security(&db).await;
        db.close().await;
        result
    }
    .await;
    sqlx::query(AssertSqlSafe(format!("DROP DATABASE `{name}`")))
        .execute(&mut control)
        .await?;
    checked
}

async fn check_security(db: &MySqlPool) -> anyhow::Result<()> {
    let migrator = sqlx::migrate!();
    migrator.run_to(11, db).await?;
    let old_password = "legacy-password-for-test";
    let old_hash = auth::hash(old_password.to_owned())
        .await
        .map_err(|error| anyhow::anyhow!(error.1))?;
    sqlx::query(
        "INSERT INTO users(id,email,name,password,admin) VALUES
                (1,'buyer@example.test','Buyer',?,FALSE),
                (2,'admin@example.test','Admin',?,TRUE),
                (3,'admin@seed.tosca.invalid','Fixture',?,TRUE)",
    )
    .bind(&old_hash)
    .bind(&old_hash)
    .bind(&old_hash)
    .execute(db)
    .await?;
    sqlx::query("UPDATE users SET name='Current buyer' WHERE id=1")
        .execute(db)
        .await?;
    sqlx::raw_sql(
        "INSERT INTO shops(id,name,owner_id) VALUES (1,'Shop',2);
         INSERT INTO members(shop_id,user_id) VALUES (1,2);
         INSERT INTO products(id,shop_id,name,description,status,actor_id) VALUES (1,1,'Product','Test','active',2);
         INSERT INTO variants(id,product_id,name,price,stock,grams,actor_id) VALUES (1,1,'Standard',1000,10,100,2);
         INSERT INTO addresses(id,user_id,label,detail) VALUES (1,1,'Home','Test address');",
    )
    .execute(db)
    .await?;
    migrator.run(db).await?;
    let app = App {
        db: db.clone(),
        secret: "local disposable test signing key".into(),
        search: search::Search::new(false, embeddings::Embedder::new(false))?,
        recommendations: recommendations::Recommendations::new(),
        tax: 0,
        payment_hold_hours: 24,
        allow_test_accounts: false,
        app_url: Arc::new(storefront_url("https://market.example.test")?),
    };
    let router = auth::routes()
        .merge(commerce::routes())
        .merge(operations::routes())
        .layer(middleware::from_fn_with_state(
            app.clone(),
            require_same_origin_write,
        ))
        .with_state(app.clone());
    let login = |email: &'static str, password: &'static str| {
        request(
            &router,
            "POST",
            "/auth/login",
            None,
            json!({ "email":email, "password":password }),
        )
    };
    let (status, body, buyer_headers) = login("buyer@example.test", old_password).await?;
    anyhow::ensure!(status == StatusCode::OK && body == json!({"ok":true}));
    let buyer = secure_session(&buyer_headers)?;
    let admin = secure_session(&login("admin@example.test", old_password).await?.2)?;
    anyhow::ensure!(
        login("admin@seed.tosca.invalid", old_password).await?.0 == StatusCode::UNAUTHORIZED
    );
    let add = || {
        request(
            &router,
            "POST",
            "/cart/1",
            Some(&buyer),
            json!({ "quantity":1 }),
        )
    };
    let (first, second) = tokio::try_join!(add(), add())?;
    anyhow::ensure!(first.0 == StatusCode::OK && second.0 == StatusCode::OK);
    let quantity: i32 =
        sqlx::query_scalar("SELECT quantity FROM carts WHERE user_id=1 AND variant_id=1")
            .fetch_one(db)
            .await?;
    anyhow::ensure!(quantity == 2);

    let key = uuid::Uuid::new_v4().simple().to_string();
    let basket = json!({"address":1,"items":[{"id":1,"quantity":2}],"key":key});
    let (status, checkout, _) =
        request(&router, "POST", "/checkout", Some(&buyer), basket.clone()).await?;
    anyhow::ensure!(status == StatusCode::OK, "{checkout}");
    let retry = request(&router, "POST", "/checkout", Some(&buyer), basket).await?;
    anyhow::ensure!(retry.1 == checkout);
    let id = checkout["id"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("missing checkout id"))?;
    let order: i64 = sqlx::query_scalar("SELECT id FROM orders WHERE checkout_id=?")
        .bind(id)
        .fetch_one(db)
        .await?;
    let order_url = format!("/merchant/1/orders/{order}");
    let (status, merchant_order, _) =
        request(&router, "GET", &order_url, Some(&admin), Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK && merchant_order["address"].is_null());
    sqlx::query("UPDATE checkouts SET created_at=CURRENT_TIMESTAMP(6)-INTERVAL 25 HOUR WHERE id=?")
        .bind(id)
        .execute(db)
        .await?;
    let pay = request(
        &router,
        "POST",
        &format!("/checkout/{id}/pay"),
        Some(&admin),
        Value::Null,
    )
    .await?;
    anyhow::ensure!(pay.0 == StatusCode::CONFLICT);
    sqlx::query("UPDATE variants SET stock=100000000 WHERE id=1")
        .execute(db)
        .await?;
    let (first, second) = tokio::join!(
        commerce::expire_checkout(&app, id),
        commerce::expire_checkout(&app, id)
    );
    anyhow::ensure!(first.is_ok() && second.is_ok(), "expiry failed");
    let stock: i64 = sqlx::query_scalar("SELECT stock FROM variants WHERE id=1")
        .fetch_one(db)
        .await?;
    let payment: String = sqlx::query_scalar("SELECT payment FROM checkouts WHERE id=?")
        .bind(id)
        .fetch_one(db)
        .await?;
    anyhow::ensure!(stock == 100_000_002 && payment == "cancelled");

    let new_password = "replacement-password-for-test";
    let updated = request(
        &router,
        "PUT",
        "/admin/users/1",
        Some(&admin),
        json!({"password":new_password}),
    )
    .await?;
    anyhow::ensure!(updated.0 == StatusCode::OK);
    let old_session = request(&router, "GET", "/auth/me", Some(&buyer), Value::Null).await?;
    anyhow::ensure!(old_session.0 == StatusCode::UNAUTHORIZED);
    anyhow::ensure!(login("buyer@example.test", old_password).await?.0 == StatusCode::UNAUTHORIZED);
    anyhow::ensure!(login("buyer@example.test", new_password).await?.0 == StatusCode::OK);
    let historical: Vec<(String, String)> = sqlx::query_as(
        "SELECT name,password FROM users FOR SYSTEM_TIME ALL WHERE id=1 ORDER BY ROW_START",
    )
    .fetch_all(db)
    .await?;
    anyhow::ensure!(
        historical
            == [
                ("Buyer".into(), "!".into()),
                ("Current buyer".into(), "!".into())
            ]
    );
    let Json(visible) = search::visible_hits(db, &[1], &[1])
        .await
        .map_err(|error| anyhow::anyhow!(error.1))?;
    anyhow::ensure!(visible["items"][0]["lowest_price"] == 1000 && visible["shops"][0]["id"] == 1);
    sqlx::query("UPDATE shops SET suspended=TRUE WHERE id=1")
        .execute(db)
        .await?;
    let Json(hidden) = search::visible_hits(db, &[1], &[1])
        .await
        .map_err(|error| anyhow::anyhow!(error.1))?;
    anyhow::ensure!(hidden == json!({"items":[],"shops":[]}));
    check_session_routes(&app, &router).await?;
    Ok(())
}

fn secure_session(headers: &[String]) -> anyhow::Result<String> {
    anyhow::ensure!(headers.len() == 3);
    let mut pairs = Vec::new();
    for header in headers {
        anyhow::ensure!(header.contains("; SameSite=Strict") && header.contains("; Secure"));
        let (pair, _) = header
            .split_once(';')
            .ok_or_else(|| anyhow::anyhow!("missing cookie attributes"))?;
        if pair.starts_with("__Host-") {
            anyhow::ensure!(header.contains("; HttpOnly") && header.contains("; Path=/;"));
        } else {
            anyhow::ensure!(pair.starts_with("tosca_session=") && !header.contains("; HttpOnly"));
        }
        pairs.push(pair);
    }
    Ok(pairs.join("; "))
}

async fn check_session_routes(app: &App, router: &Router) -> anyhow::Result<()> {
    let guest: i64 = sqlx::query(
        "INSERT INTO users(name,email,guest) VALUES ('Legacy guest','old-guest@example.test',TRUE)",
    )
    .execute(&app.db)
    .await?
    .last_insert_id()
    .try_into()?;
    let sid = uuid::Uuid::new_v4().to_string();
    let legacy = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":guest,"sid":sid,"kind":"refresh","jti":"test-legacy","exp":time::OffsetDateTime::now_utc().unix_timestamp()+900}),
        &jsonwebtoken::EncodingKey::from_secret(app.secret.as_bytes())?,
    )?;
    sqlx::query("INSERT INTO sessions(id,user_id,refresh_hash,expires_at) VALUES (?,?,?,UTC_TIMESTAMP()+INTERVAL 15 MINUTE)")
        .bind(&sid).bind(guest).bind(auth::digest(&legacy)).execute(&app.db).await?;
    let path = "/auth/session-upgrade";
    let (status, body, headers) =
        request(router, "POST", path, None, json!({"refresh":legacy})).await?;
    anyhow::ensure!(status == StatusCode::OK && body == json!({"ok":true}));
    let cookies = secure_session(&headers)?;
    anyhow::ensure!(cookies.contains(&format!("tosca_session=guest.{sid}.")));
    anyhow::ensure!(
        request(router, "POST", path, None, json!({"refresh":legacy}))
            .await?
            .0
            == StatusCode::UNAUTHORIZED
    );

    let denied = http::Request::post("/auth/logout")
        .header(http::header::COOKIE, &cookies)
        .header("x-tosca-request", "1")
        .header(http::header::ORIGIN, "https://another.example.test")
        .body(Body::empty())?;
    anyhow::ensure!(router.clone().oneshot(denied).await?.status() == StatusCode::FORBIDDEN);
    let denied = http::Request::post("/auth/logout")
        .header(http::header::COOKIE, &cookies)
        .body(Body::empty())?;
    anyhow::ensure!(router.clone().oneshot(denied).await?.status() == StatusCode::FORBIDDEN);

    let (status, _, rotated) =
        request(router, "POST", "/auth/refresh", Some(&cookies), Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK);
    let fresh = secure_session(&rotated)?;
    anyhow::ensure!(fresh != cookies);
    let (status, body, _) = request(router, "GET", "/auth/me", Some(&fresh), Value::Null).await?;
    anyhow::ensure!(status == StatusCode::OK && body["user"]["id"] == guest);
    let (status, _, cleared) =
        request(router, "POST", "/auth/logout", Some(&fresh), Value::Null).await?;
    anyhow::ensure!(
        status == StatusCode::OK
            && cleared.len() == 3
            && cleared.iter().all(|cookie| cookie.contains("; Max-Age=0"))
    );
    anyhow::ensure!(
        request(router, "GET", "/auth/me", Some(&fresh), Value::Null)
            .await?
            .0
            == StatusCode::UNAUTHORIZED
    );
    Ok(())
}

async fn request(
    router: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Value,
) -> anyhow::Result<(StatusCode, Value, Vec<String>)> {
    let mut request = http::Request::builder()
        .method(method)
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .header("x-tosca-request", "1")
        .header(http::header::ORIGIN, "https://market.example.test");
    if let Some(token) = token {
        request = request.header(http::header::COOKIE, token);
    }
    let mut request = request.body(Body::from(body.to_string()))?;
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((Ipv4Addr::LOCALHOST, 50000))));
    let response = router.clone().oneshot(request).await?;
    let status = response.status();
    let cookies = response
        .headers()
        .get_all(http::header::SET_COOKIE)
        .iter()
        .map(|header| header.to_str().map(str::to_owned))
        .collect::<std::result::Result<_, _>>()?;
    let bytes = to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, serde_json::from_slice(&bytes)?, cookies))
}
