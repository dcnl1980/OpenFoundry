pub mod streams;
pub mod tenant;
pub mod topologies;

use auth_middleware::Claims;
use axum::{http::StatusCode, Json};
use serde::Serialize;
use sqlx::{Postgres, Transaction};

use crate::AppState;

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
	tracing::error!("streaming-service database error: {cause}");
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