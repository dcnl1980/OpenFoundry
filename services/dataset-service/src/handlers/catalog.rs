use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use auth_middleware::layer::AuthUser;

use crate::{domain::catalog, AppState};

use super::tenant::begin_scope;

/// GET /api/v1/datasets/catalog/facets
pub async fn get_catalog_facets(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match catalog::fetch_catalog_facets(&mut tx).await {
        Ok(facets) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("list catalog facets commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(facets).into_response()
        }
        Err(error) => {
            tracing::error!("list catalog facets failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
