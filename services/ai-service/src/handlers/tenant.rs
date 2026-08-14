use auth_middleware::{begin_tenant_transaction, Claims};
use axum::response::Response;
use sqlx::{Postgres, Transaction};

use crate::AppState;

pub async fn begin_scope<'a>(
    state: &'a AppState,
    claims: &Claims,
) -> Result<Transaction<'a, Postgres>, Response> {
    let mut tx = begin_tenant_transaction(&state.db, claims.tenant_scope_id())
        .await
        .map_err(|error| {
            tracing::error!("ai-service tenant transaction failed: {error}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": "tenant scope failed" })),
            )
                .into_response()
        })?;
    sqlx::query("SELECT openfoundry_clone_system_ai_catalog()")
        .execute(&mut *tx)
        .await
        .map_err(|error| {
            tracing::error!("ai-service catalog clone failed: {error}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": "tenant scope failed" })),
            )
                .into_response()
        })?;
    Ok(tx)
}

use axum::response::IntoResponse;
