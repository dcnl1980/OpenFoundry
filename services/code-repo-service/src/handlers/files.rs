use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use auth_middleware::layer::AuthUser;

use crate::{
	domain::search,
	handlers::{commit_scope, db_error, load_files, load_repository_row, not_found, open_scope, ServiceResult},
	models::{file::{RepositoryFile, SearchResponse}, ListResponse},
	AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
	pub q: Option<String>,
}

pub async fn list_files(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<RepositoryFile>> {
	let mut tx = open_scope(&state, &claims).await?;
	load_repository_row(&mut *tx, id).await.map_err(|cause| db_error(&cause))?.ok_or_else(|| not_found("repository not found"))?;
	let files = load_files(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse { items: files }))
}

pub async fn search_files(
	Path(id): Path<uuid::Uuid>,
	Query(query): Query<SearchQuery>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<SearchResponse> {
	let mut tx = open_scope(&state, &claims).await?;
	load_repository_row(&mut *tx, id).await.map_err(|cause| db_error(&cause))?.ok_or_else(|| not_found("repository not found"))?;
	let files = load_files(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let query_text = query.q.unwrap_or_else(|| "package".to_string());
	let results = search::search(&files, &query_text);
	Ok(Json(SearchResponse { query: query_text, results }))
}
