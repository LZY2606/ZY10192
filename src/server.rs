use crate::analysis::{analyze, AnalysisRequest};
use crate::db::{AppState, Branch, ImportReport, Snapshot};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({ "error": self.message });
        Response::builder()
            .status(self.status)
            .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(body.to_string()))
            .unwrap()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::bad_request(error)
    }
}

type ApiResult<T> = Result<T, ApiError>;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/static/app.js", get(app_js))
        .route("/static/style.css", get(style_css))
        .route("/api/health", get(health))
        .route("/api/constants", get(constants))
        .route("/api/fixture", get(fixture))
        .route("/api/datasets", get(list_datasets))
        .route("/api/datasets/{id}", get(get_dataset))
        .route("/api/analyze", post(analyze_endpoint))
        .route(
            "/api/branches/{dataset_id}",
            get(list_branches).post(create_branch),
        )
        .route("/api/runs", get(list_runs))
        .route("/api/snapshot", get(export_snapshot).post(import_snapshot))
        .route("/api/reset", post(reset_to_fixtures))
        .with_state(state)
}

async fn index() -> Response {
    content_response(
        "text/html; charset=utf-8",
        include_str!("../static/index.html"),
    )
}

async fn app_js() -> Response {
    content_response(
        "text/javascript; charset=utf-8",
        include_str!("../static/app.js"),
    )
}

async fn style_css() -> Response {
    content_response(
        "text/css; charset=utf-8",
        include_str!("../static/style.css"),
    )
}

fn content_response(content_type: &str, body: &'static str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .unwrap()
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok","service":"协和线交台"}))
}

async fn constants(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    let _ = &state;
    Ok(Json(serde_json::json!({
        "constants": crate::analysis::constants_fingerprint()
    })))
}

async fn fixture() -> Json<serde_json::Value> {
    let datasets: serde_json::Value =
        serde_json::from_str(crate::FIXTURE_JSON).expect("bundled fixture is valid JSON");
    Json(serde_json::json!({ "fixture_json": crate::FIXTURE_JSON, "datasets": datasets }))
}

async fn list_datasets(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        serde_json::json!({ "datasets": state.list_datasets()? }),
    ))
}

async fn get_dataset(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let dataset = state.get_dataset(&id)?.ok_or_else(|| ApiError {
        status: StatusCode::NOT_FOUND,
        message: "数据集不存在".into(),
    })?;
    Ok(Json(serde_json::json!({ "dataset": dataset })))
}

#[derive(serde::Deserialize)]
struct AnalyzePayload {
    request: AnalysisRequest,
    save_run: Option<bool>,
    branch_id: Option<String>,
}

async fn analyze_endpoint(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AnalyzePayload>,
) -> ApiResult<Json<serde_json::Value>> {
    let dataset = state
        .get_dataset(&payload.request.dataset_id)?
        .ok_or_else(|| ApiError::bad_request("数据集不存在"))?;
    let request = payload.request;
    let response = analyze(request.clone(), &dataset)?;
    let run_id = if payload.save_run.unwrap_or(false) {
        Some(state.save_run(&request, &response, payload.branch_id)?)
    } else {
        None
    };
    Ok(Json(
        serde_json::json!({ "analysis": response, "run_id": run_id }),
    ))
}

async fn list_branches(
    State(state): State<Arc<AppState>>,
    Path(dataset_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        serde_json::json!({ "branches": state.list_branches(&dataset_id)? }),
    ))
}

async fn create_branch(
    State(state): State<Arc<AppState>>,
    Path(dataset_id): Path<String>,
    Json(mut branch): Json<Branch>,
) -> ApiResult<Json<serde_json::Value>> {
    branch.dataset_id = dataset_id;
    let saved = state.create_branch(branch)?;
    Ok(Json(serde_json::json!({ "branch": saved })))
}

async fn list_runs(State(state): State<Arc<AppState>>) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({ "runs": state.list_runs()? })))
}

async fn export_snapshot(State(state): State<Arc<AppState>>) -> ApiResult<Json<Snapshot>> {
    Ok(Json(state.export_snapshot()?))
}

async fn import_snapshot(
    State(state): State<Arc<AppState>>,
    Json(snapshot): Json<Snapshot>,
) -> ApiResult<Json<ImportReport>> {
    Ok(Json(state.import_snapshot(&snapshot)?))
}

#[derive(serde::Deserialize)]
struct ResetPayload {
    fixture_json: String,
}

async fn reset_to_fixtures(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ResetPayload>,
) -> ApiResult<Json<serde_json::Value>> {
    state.clear_all()?;
    state.seed_if_empty(&payload.fixture_json)?;
    Ok(Json(serde_json::json!({"status":"reset_to_fixed_fixture"})))
}
