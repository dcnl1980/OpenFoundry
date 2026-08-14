use auth_middleware::{begin_tenant_transaction, Claims};
use axum::response::Response;
use sqlx::{Postgres, Transaction};

use crate::AppState;

pub async fn begin_scope<'a>(
    state: &'a AppState,
    claims: &Claims,
) -> Result<Transaction<'a, Postgres>, Response> {
    begin_tenant_transaction(&state.db, claims.tenant_scope_id())
        .await
        .map_err(|error| {
            tracing::error!("app-builder-service tenant transaction failed: {error}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": "tenant scope failed" })),
            )
                .into_response()
        })
}

use axum::response::IntoResponse;
