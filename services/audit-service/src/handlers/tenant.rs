use auth_middleware::{begin_tenant_transaction, Claims};
use axum::{http::StatusCode, Json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::handlers::ErrorResponse;
use crate::AppState;

pub async fn begin_scope<'a>(
    state: &'a AppState,
    claims: &Claims,
) -> Result<Transaction<'a, Postgres>, (StatusCode, Json<ErrorResponse>)> {
    begin_tenant_id(state, claims.tenant_scope_id()).await
}

pub async fn begin_tenant_id<'a>(
    state: &'a AppState,
    tenant_id: Uuid,
) -> Result<Transaction<'a, Postgres>, (StatusCode, Json<ErrorResponse>)> {
    begin_tenant_transaction(&state.db, tenant_id)
        .await
        .map_err(|error| {
            tracing::error!("audit-service tenant transaction failed: {error}");
            crate::handlers::internal_error("tenant scope failed")
        })
}
