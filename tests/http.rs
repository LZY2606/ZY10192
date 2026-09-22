use concordia_crossing::db::Database;
use concordia_crossing::web;
use std::sync::Arc;
use tower::ServiceExt;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};

fn app() -> axum::Router {
    let database = Arc::new(Database::in_memory().unwrap());
    database.seed_fixtures().unwrap();
    web::router(database)
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn index_shows_chinese_title() {
    let response = app()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("<title>协和线交台</title>"));
    assert!(body.contains("协和线交台"));
}

#[tokio::test]
async fn clear_export_then_import_reproduces_runs() {
    let application = app();
    let export_response = application
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(export_response.status(), StatusCode::OK);
    let export_body = body_string(export_response).await;
    assert!(export_body.contains("fixture-tangent"));

    let clear_response = application
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs/clear")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(clear_response.status(), StatusCode::OK);
    let empty = body_string(clear_response).await;
    assert!(empty.contains("\"deleted\":3"));

    let import_response = application
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/import")
                .header("content-type", "application/json")
                .body(Body::from(export_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(import_response.status(), StatusCode::OK);
    let imported = body_string(import_response).await;
    assert!(imported.contains("\"inserted\":3"));
    assert!(imported.contains("tangent_double_root"));
}

#[tokio::test]
async fn branch_endpoint_excludes_selected_point() {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/runs/fixture-tangent/branches")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"excluded_point_ids":["T-01"]}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_string(response).await;
    assert!(body.contains("T-01"));
    assert!(body.contains("included_count"));
}
