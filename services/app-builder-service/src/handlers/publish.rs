use auth_middleware::layer::AuthUser;
use axum::{
	extract::{Path, State},
	Json,
};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::{
	handlers::{db_error, load_app, scoped_tx, ServiceResult},
	models::version::{AppVersion, AppVersionRow, ListAppVersionsResponse, PublishAppRequest},
	AppState,
};

pub async fn list_versions(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(app_id): Path<Uuid>,
) -> ServiceResult<Json<ListAppVersionsResponse>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	load_app(&mut tx, app_id).await?;

	let rows = sqlx::query_as::<_, AppVersionRow>(
		"SELECT * FROM app_versions WHERE app_id = $1 ORDER BY version_number DESC",
	)
	.bind(app_id)
	.fetch_all(&mut *tx)
	.await
	.map_err(db_error)?;
	tx.commit().await.map_err(db_error)?;

	Ok(Json(ListAppVersionsResponse {
		data: rows.into_iter().map(Into::into).collect(),
	}))
}

pub async fn publish_app(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(app_id): Path<Uuid>,
	Json(request): Json<PublishAppRequest>,
) -> ServiceResult<Json<AppVersion>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let app = load_app(&mut tx, app_id).await?;
	let snapshot = app.snapshot();
	let version_id = Uuid::now_v7();
	let notes = request.notes.unwrap_or_default();
	let tenant_id = claims.tenant_scope_id();

	let version_number: i32 = sqlx::query_scalar(
		"SELECT COALESCE(MAX(version_number), 0) + 1
		 FROM app_versions
		 WHERE app_id = $1",
	)
	.bind(app_id)
	.fetch_one(&mut *tx)
	.await
	.map_err(db_error)?;

	let version = sqlx::query_as::<_, AppVersionRow>(
		"INSERT INTO app_versions (
			id, app_id, version_number, status, app_snapshot, notes, created_by, published_at, tenant_id
		 )
		 VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), $8)
		 RETURNING *",
	)
	.bind(version_id)
	.bind(app_id)
	.bind(version_number)
	.bind("published")
	.bind(SqlJson(snapshot))
	.bind(notes)
	.bind(Option::<Uuid>::None)
	.bind(tenant_id)
	.fetch_one(&mut *tx)
	.await
	.map_err(db_error)?;

	sqlx::query(
		"UPDATE apps
		 SET published_version_id = $2,
			 status = 'published',
			 updated_at = NOW()
		 WHERE id = $1",
	)
	.bind(app_id)
	.bind(version_id)
	.execute(&mut *tx)
	.await
	.map_err(db_error)?;

	tx.commit().await.map_err(db_error)?;

	Ok(Json(version.into()))
}
