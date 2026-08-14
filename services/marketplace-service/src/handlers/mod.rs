pub mod browse;
pub mod install;
pub mod publish;
pub mod reviews;
pub mod tenant;

use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::models::{
	install::InstallRow,
	listing::ListingRow,
	package::PackageVersionRow,
	review::ReviewRow,
};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
	pub error: String,
}

pub type ServiceResult<T> = Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(StatusCode::BAD_REQUEST, Json(ErrorResponse { error: message.into() }))
}

pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(StatusCode::NOT_FOUND, Json(ErrorResponse { error: message.into() }))
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: message.into() }))
}

pub fn db_error(cause: &sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
	tracing::error!("marketplace-service database error: {cause}");
	internal_error("database operation failed")
}

pub async fn open_scope<'a>(
	state: &'a crate::AppState,
	claims: &auth_middleware::Claims,
) -> Result<sqlx::Transaction<'a, sqlx::Postgres>, (StatusCode, Json<ErrorResponse>)> {
	tenant::begin_scope(state, claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))
}

pub async fn commit_scope(
	tx: sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
	tx.commit().await.map_err(|cause| db_error(&cause))
}

pub async fn load_listing_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<ListingRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, ListingRow>("SELECT * FROM marketplace_listings WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_listings<'e, E>(db: E) -> Result<Vec<crate::models::listing::ListingDefinition>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, ListingRow>(
		"SELECT * FROM marketplace_listings
		 ORDER BY install_count DESC, average_rating DESC, updated_at DESC",
	)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::listing::ListingDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_versions<'e, E>(
	db: E,
	listing_id: uuid::Uuid,
) -> Result<Vec<crate::models::package::PackageVersion>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, PackageVersionRow>(
		"SELECT * FROM marketplace_package_versions
		 WHERE listing_id = $1
		 ORDER BY published_at DESC",
	)
	.bind(listing_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::package::PackageVersion::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_reviews<'e, E>(
	db: E,
	listing_id: uuid::Uuid,
) -> Result<Vec<crate::models::review::ListingReview>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, ReviewRow>(
		"SELECT * FROM marketplace_reviews
		 WHERE listing_id = $1
		 ORDER BY created_at DESC",
	)
	.bind(listing_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::review::ListingReview::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_installs<'e, E>(db: E) -> Result<Vec<crate::models::install::InstallRecord>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, InstallRow>(
		"SELECT * FROM marketplace_installs
		 ORDER BY installed_at DESC",
	)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::install::InstallRecord::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}
