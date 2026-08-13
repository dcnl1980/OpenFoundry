use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::AppState;
use crate::models::dataset::{CreateDatasetRequest, Dataset, ListDatasetsQuery, UpdateDatasetRequest};
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

/// POST /api/v1/datasets
pub async fn create_dataset(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Json(body): Json<CreateDatasetRequest>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let id = Uuid::now_v7();
    let format = body.format.unwrap_or_else(|| "parquet".to_string());
    let storage_path = format!("datasets/{id}");
    let tags = body.tags.unwrap_or_default();
    let tenant_id = claims.tenant_scope_id();

    let result = sqlx::query_as::<_, Dataset>(
          r#"INSERT INTO datasets (id, name, description, format, storage_path, owner_id, tenant_id, tags, active_branch)
              VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'main')
           RETURNING *"#,
    )
    .bind(id)
    .bind(&body.name)
    .bind(body.description.as_deref().unwrap_or(""))
    .bind(&format)
    .bind(&storage_path)
    .bind(claims.sub)
    .bind(tenant_id)
    .bind(&tags)
    .fetch_one(&mut *tx)
    .await;

    match result {
        Ok(ds) => {
            let _ = sqlx::query(
                r#"INSERT INTO dataset_branches (
                       id, dataset_id, name, version, description, is_default
                   )
                   VALUES ($1, $2, 'main', $3, 'Default branch', TRUE)
                   ON CONFLICT (dataset_id, name) DO NOTHING"#,
            )
            .bind(Uuid::now_v7())
            .bind(ds.id)
            .bind(ds.current_version)
            .execute(&mut *tx)
            .await;
            if let Err(error) = tx.commit().await {
                tracing::error!("create dataset commit failed: {error}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "create failed" })),
                )
                    .into_response();
            }

            (StatusCode::CREATED, Json(ds)).into_response()
        }
        Err(e) => {
            tracing::error!("create dataset failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "create failed" }))).into_response()
        }
    }
}

/// GET /api/v1/datasets
pub async fn list_datasets(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Query(params): Query<ListDatasetsQuery>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * per_page;

    let search_pattern = params.search.map(|s| format!("%{s}%"));

    let datasets = sqlx::query_as::<_, Dataset>(
        r#"SELECT * FROM datasets
           WHERE ($1::TEXT IS NULL OR name ILIKE $1 OR description ILIKE $1)
             AND ($2::TEXT IS NULL OR $2 = ANY(tags))
                         AND ($3::UUID IS NULL OR owner_id = $3)
                     ORDER BY created_at DESC
                     LIMIT $4 OFFSET $5"#,
    )
    .bind(&search_pattern)
    .bind(&params.tag)
        .bind(params.owner_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&mut *tx)
    .await;

    let total = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM datasets
           WHERE ($1::TEXT IS NULL OR name ILIKE $1 OR description ILIKE $1)
                         AND ($2::TEXT IS NULL OR $2 = ANY(tags))
                         AND ($3::UUID IS NULL OR owner_id = $3)"#,
    )
    .bind(&search_pattern)
    .bind(&params.tag)
        .bind(params.owner_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0);

    match datasets {
        Ok(ds) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("list datasets commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(serde_json::json!({
            "data": ds,
            "page": page,
            "per_page": per_page,
            "total": total,
            "total_pages": (total as f64 / per_page as f64).ceil() as i64,
        })).into_response()
        }
        Err(e) => {
            tracing::error!("list datasets failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /api/v1/datasets/:id
pub async fn get_dataset(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(dataset_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let ds = sqlx::query_as::<_, Dataset>("SELECT * FROM datasets WHERE id = $1")
        .bind(dataset_id)
        .fetch_optional(&mut *tx)
        .await;

    match ds {
        Ok(Some(d)) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("get dataset commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(d).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("get dataset failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// PATCH /api/v1/datasets/:id
pub async fn update_dataset(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(dataset_id): Path<Uuid>,
    Json(body): Json<UpdateDatasetRequest>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    let result = sqlx::query_as::<_, Dataset>(
        r#"UPDATE datasets
           SET name = COALESCE($2, name),
               description = COALESCE($3, description),
               tags = COALESCE($4, tags),
               owner_id = COALESCE($5, owner_id),
               updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(dataset_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.tags)
    .bind(body.owner_id)
    .fetch_optional(&mut *tx)
    .await;

    match result {
        Ok(Some(d)) => {
            if let Err(error) = tx.commit().await {
                tracing::error!("update dataset commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            Json(d).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("update dataset failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// DELETE /api/v1/datasets/:id
pub async fn delete_dataset(
    State(state): State<AppState>,
    AuthUser(claims): AuthUser,
    Path(dataset_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    if let Ok(Some(ds)) = sqlx::query_as::<_, Dataset>("SELECT * FROM datasets WHERE id = $1")
        .bind(dataset_id)
        .fetch_optional(&mut *tx)
        .await
    {
        let _ = state.storage.delete(&ds.storage_path).await;
    }

    let result = sqlx::query("DELETE FROM datasets WHERE id = $1")
        .bind(dataset_id)
        .execute(&mut *tx)
        .await;

    match result {
        Ok(r) if r.rows_affected() > 0 => {
            if let Err(error) = tx.commit().await {
                tracing::error!("delete dataset commit failed: {error}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("delete dataset failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
