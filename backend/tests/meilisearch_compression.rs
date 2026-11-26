use std::time::Duration;

use axum::{
    Json, Router,
    http::{HeaderMap, header},
    middleware,
    response::Response,
    routing::post,
};
use meilisearch_sdk::client::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;

#[tokio::test]
async fn search_uses_plain_json_requests_and_decodes_gzip_responses() -> anyhow::Result<()> {
    let router = Router::new()
        .route(
            "/indexes/products/search",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers[header::ACCEPT_ENCODING], "gzip");
                assert!(!headers.contains_key(header::CONTENT_ENCODING));
                assert_eq!(body["q"], "sepatu futsal");
                Json(json!({
                    "hits": [{"id": 66786}],
                    "query": "sepatu futsal",
                    "processingTimeMs": 1,
                    "limit": 6,
                    "offset": 0,
                    "estimatedTotalHits": 1
                }))
            }),
        )
        .layer(CompressionLayer::new())
        .layer(middleware::map_response(|response: Response| async move {
            assert_eq!(response.headers()[header::CONTENT_ENCODING], "gzip");
            response
        }));
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let client = Client::new(format!("http://{}", listener.local_addr()?), None::<&str>)?;
    let server = tokio::spawn(async move { axum::serve(listener, router).await });

    let index = client.index("products");
    let mut query = index.search();
    query.with_query("sepatu futsal").with_limit(6);
    let response = tokio::time::timeout(Duration::from_secs(5), query.execute::<Value>()).await;
    server.abort();

    let response = response??;
    assert_eq!(response.hits.len(), 1);
    assert_eq!(response.hits[0].result["id"], 66786);
    Ok(())
}
