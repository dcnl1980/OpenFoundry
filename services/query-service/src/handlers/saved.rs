use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::AppState;
use crate::models::saved_query::{CreateSavedQueryRequest, ListQueriesQuery, SavedQuery};
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

/// POST /api/v1/queries/saved
pub async fn create_saved_query(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Json(body): Json<CreateSavedQueryRequest>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let id = Uuid::now_v7();
    let result = sqlx::query_as::<_, SavedQuery>(
        r#"INSERT INTO saved_queries (id, name, description, sql, owner_id, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING *"#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(&body.sql)
    .bind(claims.sub)
    .bind(claims.tenant_scope_id())
    .fetch_one(&mut *tx)
    .await;

    match result {
        Ok(q) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("create saved query commit failed: {error}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "create failed" })),
                )
                    .into_response();
            }
            (StatusCode::CREATED, Json(q)).into_response()
        }
        Err(e) => {
            tracing::error!("create saved query failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "create failed" })),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/queries/saved
pub async fn list_saved_queries(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Query(params): Query<ListQueriesQuery>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;
    let search_pattern = params.search.map(|s| format!("%{s}%"));

    let queries = sqlx::query_as::<_, SavedQuery>(
        r#"SELECT * FROM saved_queries
           WHERE ($1::TEXT IS NULL OR name ILIKE $1 OR description ILIKE $1)
           ORDER BY updated_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(&search_pattern)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await;

    match queries {
        Ok(qs) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("list saved queries commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(serde_json::json!({
                "data": qs,
                "page": page,
                "per_page": per_page,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!("list saved queries failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// DELETE /api/v1/queries/saved/:id
pub async fn delete_saved_query(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(query_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let result = sqlx::query("DELETE FROM saved_queries WHERE id = $1")
        .bind(query_id)
        .execute(&mut *tx)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            if let Err(error) = tx.commit().await {
                tracing::error!("delete saved query commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete saved query failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
