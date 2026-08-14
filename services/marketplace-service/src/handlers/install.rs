use axum::{extract::State, Json};
use auth_middleware::layer::AuthUser;

use crate::{
	domain::{dependency, registry},
	handlers::{
		bad_request, commit_scope, db_error, internal_error, load_installs, load_listing_row, load_versions, not_found,
		open_scope, ServiceResult,
	},
	models::{install::{CreateInstallRequest, InstallRecord}, ListResponse},
	AppState,
};

pub async fn list_installs(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<InstallRecord>> {
	let mut tx = open_scope(&state, &claims).await?;
	let installs = load_installs(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse { items: installs }))
}

pub async fn create_install(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(request): Json<CreateInstallRequest>,
) -> ServiceResult<InstallRecord> {
	let mut tx = open_scope(&state, &claims).await?;
	let listing_row = load_listing_row(&mut *tx, request.listing_id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("listing not found"))?;
	let listing = crate::models::listing::ListingDefinition::try_from(listing_row)
		.map_err(|cause| internal_error(cause.to_string()))?;
	let versions = load_versions(&mut *tx, request.listing_id).await.map_err(|cause| db_error(&cause))?;
	let version = versions
		.iter()
		.find(|entry| entry.version == request.version)
		.cloned()
		.or_else(|| registry::latest_version(&listing, &versions))
		.ok_or_else(|| bad_request("listing has no published versions"))?;
	let dependency_plan = dependency::resolve_dependencies(&version);
	let install = registry::install_preview(&listing, &crate::models::package::PackageVersion { dependencies: dependency_plan.clone(), ..version.clone() }, &request.workspace_name);
	let dependency_plan = serde_json::to_value(&dependency_plan).map_err(|cause| internal_error(cause.to_string()))?;
	let tenant_id = claims.tenant_scope_id();

	sqlx::query(
		"INSERT INTO marketplace_installs (id, listing_id, listing_name, version, workspace_name, status, dependency_plan, installed_at, ready_at, tenant_id)
		 VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $9, $10)",
	)
	.bind(install.id)
	.bind(install.listing_id)
	.bind(&install.listing_name)
	.bind(&install.version)
	.bind(&install.workspace_name)
	.bind(&install.status)
	.bind(dependency_plan)
	.bind(install.installed_at)
	.bind(install.ready_at)
	.bind(tenant_id)
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	sqlx::query("UPDATE marketplace_listings SET install_count = install_count + 1, updated_at = NOW() WHERE id = $1")
		.bind(install.listing_id)
		.execute(&mut *tx)
		.await
		.map_err(|cause| db_error(&cause))?;

	commit_scope(tx).await?;
	Ok(Json(install))
}
