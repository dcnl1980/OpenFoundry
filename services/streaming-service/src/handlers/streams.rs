use auth_middleware::layer::AuthUser;
use axum::{extract::Path, Json};
use sqlx::types::Json as SqlJson;
use uuid::Uuid;

use crate::{
	handlers::{bad_request, db_error, not_found, scoped_tx, ServiceResult},
	models::{
		stream::{
			ConnectorBinding, CreateStreamRequest, StreamDefinition, StreamRow, StreamSchema,
			UpdateStreamRequest,
		},
		window::{CreateWindowRequest, UpdateWindowRequest, WindowDefinition, WindowRow},
		ListResponse,
	},
	AppState,
};

async fn load_stream_row(
	tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
	id: Uuid,
) -> Result<StreamRow, sqlx::Error> {
	sqlx::query_as::<_, StreamRow>("SELECT * FROM streaming_streams WHERE id = $1")
		.bind(id)
		.fetch_one(&mut **tx)
		.await
}

async fn load_window_row(
	tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
	id: Uuid,
) -> Result<WindowRow, sqlx::Error> {
	sqlx::query_as::<_, WindowRow>("SELECT * FROM streaming_windows WHERE id = $1")
		.bind(id)
		.fetch_one(&mut **tx)
		.await
}

pub async fn list_streams(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<StreamDefinition>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let rows = sqlx::query_as::<_, StreamRow>(
		"SELECT * FROM streaming_streams ORDER BY created_at ASC",
	)
	.fetch_all(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(ListResponse {
		data: rows.into_iter().map(Into::into).collect(),
	}))
}

pub async fn create_stream(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
	Json(payload): Json<CreateStreamRequest>,
) -> ServiceResult<StreamDefinition> {
	if payload.name.trim().is_empty() {
		return Err(bad_request("stream name is required"));
	}

	let mut tx = scoped_tx(&state, &claims).await?;
	let stream_id = Uuid::now_v7();
	let schema = payload.schema.unwrap_or_else(StreamSchema::default);
	let binding = payload
		.source_binding
		.unwrap_or_else(ConnectorBinding::default);

	sqlx::query(
		"INSERT INTO streaming_streams (id, name, description, status, schema, source_binding, retention_hours, tenant_id)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
	)
	.bind(stream_id)
	.bind(payload.name.trim())
	.bind(payload.description.unwrap_or_default())
	.bind(payload.status.unwrap_or_else(|| "active".to_string()))
	.bind(SqlJson(schema))
	.bind(SqlJson(binding))
	.bind(payload.retention_hours.unwrap_or(72))
	.bind(claims.tenant_scope_id())
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_stream_row(&mut tx, stream_id)
		.await
		.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(row.into()))
}

pub async fn update_stream(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
	Path(id): Path<Uuid>,
	Json(payload): Json<UpdateStreamRequest>,
) -> ServiceResult<StreamDefinition> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let existing = match load_stream_row(&mut tx, id).await {
		Ok(row) => row,
		Err(sqlx::Error::RowNotFound) => return Err(not_found("stream not found")),
		Err(cause) => return Err(db_error(&cause)),
	};

	let schema = payload.schema.unwrap_or(existing.schema.0);
	let binding = payload.source_binding.unwrap_or(existing.source_binding.0);

	sqlx::query(
		"UPDATE streaming_streams
		 SET name = $2,
		     description = $3,
		     status = $4,
		     schema = $5,
		     source_binding = $6,
		     retention_hours = $7,
		     updated_at = now()
		 WHERE id = $1",
	)
	.bind(id)
	.bind(payload.name.unwrap_or(existing.name))
	.bind(payload.description.unwrap_or(existing.description))
	.bind(payload.status.unwrap_or(existing.status))
	.bind(SqlJson(schema))
	.bind(SqlJson(binding))
	.bind(payload.retention_hours.unwrap_or(existing.retention_hours))
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_stream_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(row.into()))
}

pub async fn list_windows(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<WindowDefinition>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let rows = sqlx::query_as::<_, WindowRow>(
		"SELECT * FROM streaming_windows ORDER BY created_at ASC",
	)
	.fetch_all(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(ListResponse {
		data: rows.into_iter().map(Into::into).collect(),
	}))
}

pub async fn create_window(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
	Json(payload): Json<CreateWindowRequest>,
) -> ServiceResult<WindowDefinition> {
	if payload.name.trim().is_empty() {
		return Err(bad_request("window name is required"));
	}

	let mut tx = scoped_tx(&state, &claims).await?;
	let window_id = Uuid::now_v7();

	sqlx::query(
		"INSERT INTO streaming_windows (
		    id, name, description, status, window_type, duration_seconds, slide_seconds,
		    session_gap_seconds, allowed_lateness_seconds, aggregation_keys, measure_fields, tenant_id
		 ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
	)
	.bind(window_id)
	.bind(payload.name.trim())
	.bind(payload.description.unwrap_or_default())
	.bind(payload.status.unwrap_or_else(|| "active".to_string()))
	.bind(payload.window_type.unwrap_or_else(|| "tumbling".to_string()))
	.bind(payload.duration_seconds.unwrap_or(300))
	.bind(payload.slide_seconds.unwrap_or(300))
	.bind(payload.session_gap_seconds.unwrap_or(180))
	.bind(payload.allowed_lateness_seconds.unwrap_or(30))
	.bind(SqlJson(payload.aggregation_keys))
	.bind(SqlJson(payload.measure_fields))
	.bind(claims.tenant_scope_id())
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_window_row(&mut tx, window_id)
		.await
		.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(row.into()))
}

pub async fn update_window(
	axum::extract::State(state): axum::extract::State<AppState>,
	AuthUser(claims): AuthUser,
	Path(id): Path<Uuid>,
	Json(payload): Json<UpdateWindowRequest>,
) -> ServiceResult<WindowDefinition> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let existing = match load_window_row(&mut tx, id).await {
		Ok(row) => row,
		Err(sqlx::Error::RowNotFound) => return Err(not_found("window not found")),
		Err(cause) => return Err(db_error(&cause)),
	};

	sqlx::query(
		"UPDATE streaming_windows
		 SET name = $2,
		     description = $3,
		     status = $4,
		     window_type = $5,
		     duration_seconds = $6,
		     slide_seconds = $7,
		     session_gap_seconds = $8,
		     allowed_lateness_seconds = $9,
		     aggregation_keys = $10,
		     measure_fields = $11,
		     updated_at = now()
		 WHERE id = $1",
	)
	.bind(id)
	.bind(payload.name.unwrap_or(existing.name))
	.bind(payload.description.unwrap_or(existing.description))
	.bind(payload.status.unwrap_or(existing.status))
	.bind(payload.window_type.unwrap_or(existing.window_type))
	.bind(payload.duration_seconds.unwrap_or(existing.duration_seconds))
	.bind(payload.slide_seconds.unwrap_or(existing.slide_seconds))
	.bind(payload.session_gap_seconds.unwrap_or(existing.session_gap_seconds))
	.bind(payload.allowed_lateness_seconds.unwrap_or(existing.allowed_lateness_seconds))
	.bind(SqlJson(
		payload
			.aggregation_keys
			.unwrap_or(existing.aggregation_keys.0),
	))
	.bind(SqlJson(payload.measure_fields.unwrap_or(existing.measure_fields.0)))
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_window_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;

	Ok(Json(row.into()))
}
