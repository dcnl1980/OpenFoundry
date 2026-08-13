use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::models::session::*;
use crate::AppState;
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

pub async fn create_session(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(notebook_id): Path<Uuid>,
    Json(body): Json<CreateSessionRequest>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let id = Uuid::now_v7();
    let kernel = body.kernel.unwrap_or_else(|| "python".to_string());

    if let Err(error) = state.kernel_manager.ensure_session(id, &kernel).await {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }

    let result = sqlx::query_as::<_, Session>(
        r#"INSERT INTO sessions (id, notebook_id, kernel, status, started_by)
           VALUES ($1, $2, $3, 'idle', $4)
           RETURNING *"#,
    )
    .bind(id)
    .bind(notebook_id)
    .bind(&kernel)
    .bind(claims.sub)
    .fetch_one(&mut *tx)
    .await;

    match result {
        Ok(s) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("create session commit failed: {error}");
                return (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response();
            }
            (StatusCode::CREATED, Json(serde_json::json!(s))).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_sessions(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(notebook_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let sessions = sqlx::query_as::<_, Session>(
        "SELECT * FROM sessions WHERE notebook_id = $1 ORDER BY created_at DESC",
    )
    .bind(notebook_id)
    .fetch_all(&mut *tx)
    .await
    .unwrap_or_default();

    if let Err(error) = tx.commit().await {
        tracing::error!("list sessions commit failed: {error}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    Json(serde_json::json!({ "data": sessions })).into_response()
}

pub async fn stop_session(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path((_notebook_id, session_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let result = sqlx::query_as::<_, Session>(
        "UPDATE sessions SET status = 'dead', last_activity = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await;

    match result {
        Ok(Some(s)) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("stop session commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            state.kernel_manager.drop_session(session_id).await;
            Json(serde_json::json!(s)).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
