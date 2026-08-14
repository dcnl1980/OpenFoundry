use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::lineage;
use crate::AppState;
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

pub async fn get_dataset_lineage(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(dataset_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match lineage::get_lineage_graph(&mut tx, dataset_id).await {
        Ok(graph) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("dataset lineage commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(graph).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_dataset_column_lineage(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(dataset_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match lineage::get_dataset_column_lineage(&mut tx, dataset_id).await {
        Ok(edges) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("column lineage commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(edges).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_full_lineage(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match lineage::get_full_lineage_graph(&mut tx).await {
        Ok(graph) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("full lineage commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(graph).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
