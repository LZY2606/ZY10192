use crate::analysis::solve_branch;
use crate::db::Database;
use crate::model::{ExportFile, RunInput};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub database: Arc<Database>,
}

#[derive(Debug, Deserialize)]
pub struct BranchRequest {
    excluded_point_ids: Vec<String>,
}

pub fn router(database: Arc<Database>) -> Router {
    let state = AppState { database };
    Router::new()
        .route("/", get(index))
        .route("/health", get(|| async { "ok" }))
        .route("/api/runs", get(list_runs).post(create_run))
        .route("/api/runs/{id}", get(get_run).delete(delete_run))
        .route("/api/runs/{id}/branches", post(create_branch))
        .route("/api/fixtures/reset", post(reset_fixtures))
        .route("/api/export", get(export_runs))
        .route("/api/import", post(import_runs))
        .route("/api/runs/clear", post(clear_runs))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn list_runs(State(state): State<AppState>) -> Response {
    match state.database.list_runs() {
        Ok(runs) => Json(runs).into_response(),
        Err(error) => internal_error(error),
    }
}

async fn get_run(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.database.get_run(&id) {
        Ok(Some(run)) => Json(run).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "run not found").into_response(),
        Err(error) => internal_error(error),
    }
}

async fn create_run(State(state): State<AppState>, Json(input): Json<RunInput>) -> Response {
    match validate_input(&input).and_then(|_| {
        state
            .database
            .upsert_run(input)
            .map_err(|error| error.to_string())
    }) {
        Ok(_) => match state.database.list_runs() {
            Ok(runs) => (StatusCode::CREATED, Json(runs)).into_response(),
            Err(error) => internal_error(error),
        },
        Err(error) => bad_request(error),
    }
}

async fn delete_run(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.database.delete_run(&id) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => (StatusCode::NOT_FOUND, "run not found").into_response(),
        Err(error) => internal_error(error),
    }
}

async fn create_branch(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<BranchRequest>,
) -> Response {
    let Ok(Some(input)) = state.database.get_input(&id) else {
        return (StatusCode::NOT_FOUND, "run not found").into_response();
    };
    let diagnostics: Vec<_> = input
        .points
        .iter()
        .map(crate::geometry::diagnose_input_point)
        .collect();
    let name = if request.excluded_point_ids.is_empty() {
        "全有效点基线".to_string()
    } else {
        format!("排除：{}", request.excluded_point_ids.join(", "))
    };
    let branch = solve_branch(&name, &input, &diagnostics, request.excluded_point_ids);
    Json(branch).into_response()
}

async fn reset_fixtures(State(state): State<AppState>) -> Response {
    state.database.clear().ok();
    match state.database.seed_fixtures() {
        Ok(_) => match state.database.list_runs() {
            Ok(runs) => Json(runs).into_response(),
            Err(error) => internal_error(error),
        },
        Err(error) => internal_error(error),
    }
}

async fn export_runs(State(state): State<AppState>) -> Response {
    match state.database.export() {
        Ok(export) => (
            [
                (
                    header::CONTENT_TYPE,
                    "application/json; charset=utf-8".to_string(),
                ),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"concordia-runs.json\"".to_string(),
                ),
            ],
            Json(export),
        )
            .into_response(),
        Err(error) => internal_error(error),
    }
}

async fn import_runs(State(state): State<AppState>, Json(export): Json<ExportFile>) -> Response {
    if export.equation_version != crate::constants::EQUATION_VERSION {
        return bad_request(anyhow_like("导入文件的年龄方程常数版本与本服务不一致"));
    }
    for run in &export.runs {
        if let Err(error) = validate_input(run) {
            return bad_request(error);
        }
    }
    match state.database.import(export) {
        Ok(inserted) => match state.database.list_runs() {
            Ok(runs) => {
                Json(serde_json::json!({ "inserted": inserted, "runs": runs })).into_response()
            }
            Err(error) => internal_error(error),
        },
        Err(error) => internal_error(error),
    }
}

async fn clear_runs(State(state): State<AppState>) -> Response {
    match state.database.clear() {
        Ok(deleted) => Json(serde_json::json!({ "deleted": deleted })).into_response(),
        Err(error) => internal_error(error),
    }
}

fn validate_input(input: &RunInput) -> Result<(), String> {
    if input.name.trim().is_empty() {
        return Err("运行名称不能为空".to_string());
    }
    if input.points.is_empty() {
        return Err("至少需要一个点；越界点也必须保留在输入中".to_string());
    }
    Ok(())
}

fn anyhow_like(message: &str) -> String {
    message.to_string()
}

fn bad_request(message: String) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": message })),
    )
        .into_response()
}

fn internal_error(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": error.to_string() })),
    )
        .into_response()
}
