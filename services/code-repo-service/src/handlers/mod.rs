pub mod branches;
pub mod commits;
pub mod diff;
pub mod files;
pub mod integrations;
pub mod merge_requests;
pub mod repos;
pub mod tenant;

use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::models::{
	branch::BranchRow,
	comment::CommentRow,
	commit::{CiRunRow, CommitRow},
	file::FileRow,
	merge_request::MergeRequestRow,
	integration::{IntegrationRow, SyncRunRow},
	repository::RepositoryRow,
};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
	pub error: String,
}

pub type ServiceResult<T> = Result<Json<T>, (StatusCode, Json<ErrorResponse>)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::BAD_REQUEST,
		Json(ErrorResponse { error: message.into() }),
	)
}

pub fn not_found(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::NOT_FOUND,
		Json(ErrorResponse { error: message.into() }),
	)
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
	(
		StatusCode::INTERNAL_SERVER_ERROR,
		Json(ErrorResponse { error: message.into() }),
	)
}

pub fn db_error(cause: &sqlx::Error) -> (StatusCode, Json<ErrorResponse>) {
	tracing::error!("code-repo-service database error: {cause}");
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

pub async fn load_repository_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<RepositoryRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, RepositoryRow>("SELECT * FROM code_repositories WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_all_repositories<'e, E>(
	db: E,
) -> Result<Vec<crate::models::repository::RepositoryDefinition>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, RepositoryRow>("SELECT * FROM code_repositories ORDER BY updated_at DESC")
		.fetch_all(db)
		.await?;

	rows.into_iter()
		.map(crate::models::repository::RepositoryDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_branches<'e, E>(
	db: E,
	repository_id: uuid::Uuid,
) -> Result<Vec<crate::models::branch::BranchDefinition>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, BranchRow>(
		"SELECT * FROM code_repository_branches
		 WHERE repository_id = $1
		 ORDER BY is_default DESC, updated_at DESC",
	)
	.bind(repository_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::branch::BranchDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_commits<'e, E>(
	db: E,
	repository_id: uuid::Uuid,
) -> Result<Vec<crate::models::commit::CommitDefinition>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, CommitRow>(
		"SELECT * FROM code_repository_commits
		 WHERE repository_id = $1
		 ORDER BY created_at DESC",
	)
	.bind(repository_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::commit::CommitDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_files<'e, E>(
	db: E,
	repository_id: uuid::Uuid,
) -> Result<Vec<crate::models::file::RepositoryFile>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, FileRow>(
		"SELECT * FROM code_repository_files
		 WHERE repository_id = $1
		 ORDER BY path ASC",
	)
	.bind(repository_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::file::RepositoryFile::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_merge_request_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<MergeRequestRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, MergeRequestRow>("SELECT * FROM code_merge_requests WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_merge_requests<'e, E>(
	db: E,
	repository_id: Option<uuid::Uuid>,
) -> Result<Vec<crate::models::merge_request::MergeRequestDefinition>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = if let Some(repository_id) = repository_id {
		sqlx::query_as::<_, MergeRequestRow>(
			"SELECT * FROM code_merge_requests
			 WHERE repository_id = $1
			 ORDER BY updated_at DESC",
		)
		.bind(repository_id)
		.fetch_all(db)
		.await?
	} else {
		sqlx::query_as::<_, MergeRequestRow>("SELECT * FROM code_merge_requests ORDER BY updated_at DESC")
			.fetch_all(db)
			.await?
	};

	rows.into_iter()
		.map(crate::models::merge_request::MergeRequestDefinition::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_comments<'e, E>(
	db: E,
	merge_request_id: uuid::Uuid,
) -> Result<Vec<crate::models::comment::ReviewComment>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, CommentRow>(
		"SELECT * FROM code_review_comments
		 WHERE merge_request_id = $1
		 ORDER BY created_at ASC",
	)
	.bind(merge_request_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::comment::ReviewComment::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_ci_runs<'e, E>(
	db: E,
	repository_id: uuid::Uuid,
) -> Result<Vec<crate::models::commit::CiRun>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, CiRunRow>(
		"SELECT * FROM code_ci_runs
		 WHERE repository_id = $1
		 ORDER BY started_at DESC",
	)
	.bind(repository_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::commit::CiRun::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_integrations<'e, E>(
	db: E,
	repository_id: Option<uuid::Uuid>,
) -> Result<Vec<crate::models::integration::RepositoryIntegration>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = if let Some(repository_id) = repository_id {
		sqlx::query_as::<_, IntegrationRow>(
			"SELECT * FROM code_repository_integrations
			 WHERE repository_id = $1
			 ORDER BY updated_at DESC",
		)
		.bind(repository_id)
		.fetch_all(db)
		.await?
	} else {
		sqlx::query_as::<_, IntegrationRow>(
			"SELECT * FROM code_repository_integrations ORDER BY updated_at DESC",
		)
		.fetch_all(db)
		.await?
	};

	rows.into_iter()
		.map(crate::models::integration::RepositoryIntegration::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}

pub async fn load_integration_row<'e, E>(db: E, id: uuid::Uuid) -> Result<Option<IntegrationRow>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	sqlx::query_as::<_, IntegrationRow>("SELECT * FROM code_repository_integrations WHERE id = $1")
		.bind(id)
		.fetch_optional(db)
		.await
}

pub async fn load_sync_runs<'e, E>(
	db: E,
	integration_id: uuid::Uuid,
) -> Result<Vec<crate::models::integration::ExternalSyncRun>, sqlx::Error>
where
	E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
	let rows = sqlx::query_as::<_, SyncRunRow>(
		"SELECT * FROM code_repository_sync_runs
		 WHERE integration_id = $1
		 ORDER BY started_at DESC",
	)
	.bind(integration_id)
	.fetch_all(db)
	.await?;

	rows.into_iter()
		.map(crate::models::integration::ExternalSyncRun::try_from)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|cause| sqlx::Error::Decode(Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, cause))))
}
