use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::AppState;
use crate::models::version::DatasetVersion;
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

/// GET /api/v1/datasets/:id/versions
pub async fn list_versions(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(dataset_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let versions = sqlx::query_as::<_, DatasetVersion>(
        "SELECT * FROM dataset_versions WHERE dataset_id = $1 ORDER BY version DESC",
    )
    .bind(dataset_id)
    .fetch_all(&mut *tx)
    .await;

    match versions {
        Ok(v) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("list versions commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(v).into_response()
        }
        Err(e) => {
            tracing::error!("list versions failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
