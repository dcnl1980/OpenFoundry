use auth_middleware::{begin_tenant_transaction, Claims};
use axum::response::Response;
use sqlx::{Postgres, Transaction};

use crate::AppState;

use super::db_failure;

pub async fn begin_scope<'a>(
    state: &'a AppState,
    claims: &Claims,
) -> Result<Transaction<'a, Postgres>, Response> {
    begin_tenant_transaction(&state.db, claims.tenant_scope_id())
        .await
        .map_err(|error| db_failure(&error))
}
