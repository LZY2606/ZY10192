use axum::body::Body;
use axum::Router;
use concordia_jiaotai::db::AppState;
use concordia_jiaotai::server;
use concordia_jiaotai::FIXTURE_JSON;
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::util::ServiceExt;

fn app() -> Router {
    let state = Arc::new(AppState::in_memory().unwrap());
    state.seed_if_empty(FIXTURE_JSON).unwrap();
    server::router(state)
}

async fn text(response: axum::http::Response<Body>) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn index_shows_chinese_service_name() {
    let response = app()
        .oneshot(
            axum::http::Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(text(response).await.contains("协和线交台"));
}

#[tokio::test]
async fn analyze_api_returns_fixture_diagnostics() {
    let request = serde_json::json!({
        "save_run": true,
        "request": {
            "dataset_id": "wetherill-tangent",
            "source_convention": "wetherill",
            "target_convention": "wetherill",
            "included_point_ids": ["w1", "w2", "w3", "w4"],
            "scatter_model": "analytical_or_mswd"
        }
    });
    let response = app()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/analyze")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = text(response).await;
    assert!(body.contains("tangent_unstable"));
    assert!(body.contains("退化切点"));
}

#[tokio::test]
async fn constants_are_exposed_as_fixed_values() {
    let response = app()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/constants")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = text(response).await;
    assert!(body.contains("fixed-upb-constants-v1"));
    assert!(body.contains("1.55125"));
}
