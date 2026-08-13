use axum::{extract::{Path, State}, Json};
use chrono::Utc;
use auth_middleware::layer::AuthUser;

use crate::{
	handlers::{
		bad_request, commit_scope, db_error, internal_error, load_branches, load_commits, load_repository_row, not_found,
		open_scope, ServiceResult,
	},
	models::{branch::{BranchDefinition, CreateBranchRequest}, ListResponse},
	AppState,
};

pub async fn list_branches(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<BranchDefinition>> {
	let mut tx = open_scope(&state, &claims).await?;
	load_repository_row(&mut *tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("repository not found"))?;
	let branches = load_branches(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse { items: branches }))
}

pub async fn create_branch(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(request): Json<CreateBranchRequest>,
) -> ServiceResult<BranchDefinition> {
	if request.name.trim().is_empty() {
		return Err(bad_request("branch name is required"));
	}
	let mut tx = open_scope(&state, &claims).await?;
	let repository = load_repository_row(&mut *tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("repository not found"))?;
	let repository = crate::models::repository::RepositoryDefinition::try_from(repository)
		.map_err(|cause| internal_error(cause.to_string()))?;
	let commits = load_commits(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	let now = Utc::now();
	let head_sha = commits
		.iter()
		.find(|commit| commit.branch_name == request.base_branch)
		.map(|commit| commit.sha.clone())
		.unwrap_or_else(|| "init000".to_string());
	let (ahead_by, pending_reviews) = crate::domain::git::branch_metrics(
		&BranchDefinition {
			id: uuid::Uuid::nil(),
			repository_id: id,
			name: request.name.clone(),
			head_sha: head_sha.clone(),
			base_branch: Some(request.base_branch.clone()),
			is_default: false,
			protected: request.protected,
			ahead_by: 0,
			pending_reviews: 0,
			updated_at: now,
		},
		commits.len(),
	);
	let tenant_id = claims.tenant_scope_id();

	sqlx::query(
		"INSERT INTO code_repository_branches (id, repository_id, name, head_sha, base_branch, is_default, protected, ahead_by, pending_reviews, updated_at, tenant_id)
		 VALUES ($1, $2, $3, $4, $5, false, $6, $7, $8, $9, $10)",
	)
	.bind(uuid::Uuid::now_v7())
	.bind(repository.id)
	.bind(&request.name)
	.bind(&head_sha)
	.bind(&request.base_branch)
	.bind(request.protected)
	.bind(ahead_by)
	.bind(pending_reviews as i32)
	.bind(now)
	.bind(tenant_id)
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let branches = load_branches(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let branch = branches.into_iter().find(|entry| entry.name == request.name).ok_or_else(|| internal_error("created branch could not be reloaded"))?;
	Ok(Json(branch))
}
