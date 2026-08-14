use auth_middleware::{begin_tenant_transaction, Claims};
use axum::{http::StatusCode, Json};
use sqlx::{Postgres, Transaction};

use crate::AppState;

use super::{internal_error, ErrorResponse};

pub async fn begin_scope<'a>(
    state: &'a AppState,
    claims: &Claims,
) -> Result<Transaction<'a, Postgres>, (StatusCode, Json<ErrorResponse>)> {
    begin_tenant_transaction(&state.db, claims.tenant_scope_id())
        .await
        .map_err(|error| {
            tracing::error!("fusion-service tenant transaction failed: {error}");
            internal_error("tenant scope failed")
        })
}
