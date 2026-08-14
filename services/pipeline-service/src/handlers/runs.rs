use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::models::run::*;
use crate::AppState;
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

pub async fn list_runs(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(pipeline_id): Path<Uuid>,
    Query(params): Query<ListRunsQuery>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let runs = sqlx::query_as::<_, PipelineRun>(
        r#"SELECT * FROM pipeline_runs
           WHERE pipeline_id = $1
           ORDER BY started_at DESC LIMIT $2 OFFSET $3"#,
    )
    .bind(pipeline_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await
    .unwrap_or_default();

    if let Err(error) = tx.commit().await {
        tracing::error!("list pipeline runs commit failed: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Json(serde_json::json!({ "data": runs })).into_response()
}

pub async fn get_run(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path((_pipeline_id, run_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match sqlx::query_as::<_, PipelineRun>("SELECT * FROM pipeline_runs WHERE id = $1")
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
    {
        Ok(Some(r)) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("get pipeline run commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(serde_json::json!(r)).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
