pub mod consume;
pub mod contracts;
pub mod peers;
pub mod shares;
pub mod tenant;

use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::models::{
	access_grant::AccessGrantRow,
	contract::ContractRow,
	peer::PeerRow,
	share::SharedDatasetRow,
	sync_status::SyncStatusRow,
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
	tracing::error!("nexus-service database error: {cause}");
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

pub async fn load_peers<'e, E>(db: E) -> Result<Vec<crate::models::peer::PeerOrganization>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, PeerRow>("SELECT * FROM nexus_peers ORDER BY updated_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::peer::PeerOrganization::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_peer_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<PeerRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, PeerRow>("SELECT * FROM nexus_peers WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_contracts<'e, E>(db: E) -> Result<Vec<crate::models::contract::SharingContract>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, ContractRow>("SELECT * FROM nexus_contracts ORDER BY updated_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::contract::SharingContract::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_contract_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<ContractRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, ContractRow>("SELECT * FROM nexus_contracts WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_shares<'e, E>(db: E) -> Result<Vec<crate::models::share::SharedDataset>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, SharedDatasetRow>("SELECT * FROM nexus_shares ORDER BY updated_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::share::SharedDataset::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_share_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<SharedDatasetRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, SharedDatasetRow>("SELECT * FROM nexus_shares WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_access_grants<'e, E>(db: E) -> Result<Vec<crate::models::access_grant::AccessGrant>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, AccessGrantRow>("SELECT * FROM nexus_access_grants ORDER BY issued_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::access_grant::AccessGrant::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_sync_statuses<'e, E>(db: E) -> Result<Vec<crate::models::sync_status::SyncStatus>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, SyncStatusRow>("SELECT * FROM nexus_sync_statuses ORDER BY updated_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::sync_status::SyncStatus::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}
