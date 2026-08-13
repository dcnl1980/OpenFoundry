pub mod features;
pub mod geocode;
pub mod layers;
pub mod tenant;
pub mod tiles;

use auth_middleware::Claims;
use axum::{http::StatusCode, Json};
use serde::Serialize;
use sqlx::{Postgres, Transaction};

use crate::{models::layer::LayerRow, AppState};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
	pub error: String,
}

pub type ServiceResult<T> = Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::BAD_REQUEST,
		Json(ErrorResponse {
			error: message.into(),
		}),
	)
}

pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::NOT_FOUND,
		Json(ErrorResponse {
			error: message.into(),
		}),
	)
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::INTERNAL_SERVER_ERROR,
		Json(ErrorResponse {
			error: message.into(),
		}),
	)
}

pub fn db_error(cause: &sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
	tracing::error!("geospatial-service database error: {cause}");
	internal_error("database operation failed")
}

pub async fn scoped_tx<'a>(
	state: &'a AppState,
	claims: &Claims,
) -> Result<Transaction<'a, Postgres>, (StatusCode, Json<ErrorResponse>)> {
	tenant::begin_scope(state, claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))
}

pub async fn load_layer_row(
	tx: &mut Transaction<'_, Postgres>,
	id: uuid::Uuid,
) -> Result<Option<LayerRow>, sqlx::Error> {
	sqlx::query_as::<_, LayerRow>("SELECT * FROM geospatial_layers WHERE id = $1")
		.bind(id)
		.fetch_optional(&mut **tx)
		.await
}

pub async fn load_all_layers(
	tx: &mut Transaction<'_, Postgres>,
) -> Result<Vec<crate::models::layer::LayerDefinition>, sqlx::Error> {
	let rows = sqlx::query_as::<_, LayerRow>(
		"SELECT * FROM geospatial_layers ORDER BY updated_at DESC",
	)
	.fetch_all(&mut **tx)
	.await?;

	rows.into_iter()
		.map(crate::models::layer::LayerDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}