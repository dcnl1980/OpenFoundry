use auth_middleware::layer::AuthUser;
use axum::{extract::{Path, State}, Json};
use chrono::Utc;

use crate::{
	handlers::{bad_request, db_error, internal_error, load_policies, load_policy_row, tenant::begin_scope, ServiceResult},
	models::{
		data_classification::{ClassificationCatalogEntry, ClassificationLevel},
		policy::{AuditPolicy, CreatePolicyRequest, UpdatePolicyRequest},
		ListResponse,
	},
	AppState,
};

pub async fn list_classifications() -> ServiceResult<Vec<ClassificationCatalogEntry>> {
	Ok(Json(vec![
		ClassificationCatalogEntry::new(ClassificationLevel::Public, "Low sensitivity, broad export allowed"),
		ClassificationCatalogEntry::new(ClassificationLevel::Confidential, "Internal-only, export controlled"),
		ClassificationCatalogEntry::new(ClassificationLevel::Pii, "Personal data requiring masking and GDPR workflows"),
	]))
}

pub async fn list_policies(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<AuditPolicy>> {
	let mut tx = begin_scope(&state, &claims).await?;
	let policies = load_policies(&mut tx).await.map_err(|cause| db_error(&cause))?;
	Ok(Json(ListResponse { items: policies }))
}

pub async fn create_policy(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(request): Json<CreatePolicyRequest>,
) -> ServiceResult<AuditPolicy> {
	if request.name.trim().is_empty() {
		return Err(bad_request("policy name is required"));
	}
	let id = uuid::Uuid::now_v7();
	let now = Utc::now();
	let rules = serde_json::to_value(&request.rules).map_err(|cause| internal_error(cause.to_string()))?;
	let mut tx = begin_scope(&state, &claims).await?;

	sqlx::query(
		"INSERT INTO audit_policies (id, name, description, scope, classification, retention_days, legal_hold, purge_mode, active, rules, updated_by, created_at, updated_at, tenant_id)
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb, $11, $12, $13, $14)",
	)
	.bind(id)
	.bind(&request.name)
	.bind(&request.description)
	.bind(&request.scope)
	.bind(request.classification.as_str())
	.bind(request.retention_days)
	.bind(request.legal_hold)
	.bind(&request.purge_mode)
	.bind(request.active)
	.bind(rules)
	.bind(&request.updated_by)
	.bind(now)
	.bind(now)
	.bind(claims.tenant_scope_id())
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_policy_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| internal_error("created policy could not be reloaded"))?;
	let policy = AuditPolicy::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;
	Ok(Json(policy))
}

pub async fn update_policy(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(request): Json<UpdatePolicyRequest>,
) -> ServiceResult<AuditPolicy> {
	let mut tx = begin_scope(&state, &claims).await?;
	let row = load_policy_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| crate::handlers::not_found("policy not found"))?;
	let mut policy = AuditPolicy::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;

	if let Some(name) = request.name { policy.name = name; }
	if let Some(description) = request.description { policy.description = description; }
	if let Some(scope) = request.scope { policy.scope = scope; }
	if let Some(classification) = request.classification { policy.classification = classification; }
	if let Some(retention_days) = request.retention_days { policy.retention_days = retention_days; }
	if let Some(legal_hold) = request.legal_hold { policy.legal_hold = legal_hold; }
	if let Some(purge_mode) = request.purge_mode { policy.purge_mode = purge_mode; }
	if let Some(active) = request.active { policy.active = active; }
	if let Some(rules) = request.rules { policy.rules = rules; }
	if let Some(updated_by) = request.updated_by { policy.updated_by = updated_by; }

	let now = Utc::now();
	let rules = serde_json::to_value(&policy.rules).map_err(|cause| internal_error(cause.to_string()))?;

	sqlx::query(
		"UPDATE audit_policies
		 SET name = $2, description = $3, scope = $4, classification = $5, retention_days = $6, legal_hold = $7, purge_mode = $8, active = $9, rules = $10::jsonb, updated_by = $11, updated_at = $12
		 WHERE id = $1",
	)
	.bind(id)
	.bind(&policy.name)
	.bind(&policy.description)
	.bind(&policy.scope)
	.bind(policy.classification.as_str())
	.bind(policy.retention_days)
	.bind(policy.legal_hold)
	.bind(&policy.purge_mode)
	.bind(policy.active)
	.bind(rules)
	.bind(&policy.updated_by)
	.bind(now)
	.execute(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;

	let row = load_policy_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| internal_error("updated policy could not be reloaded"))?;
	let policy = AuditPolicy::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;
	Ok(Json(policy))
}
