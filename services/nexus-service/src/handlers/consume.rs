use std::collections::HashMap;

use axum::{extract::State, Json};
use auth_middleware::layer::AuthUser;

use crate::{
	domain::{audit_bridge, federation, replication, schema_compat},
	handlers::{
		commit_scope, db_error, internal_error, load_access_grants, load_contracts, load_peers, load_shares,
		load_sync_statuses, not_found, open_scope, ServiceResult,
	},
	models::{
		access_grant::{FederatedQueryRequest, FederatedQueryResult},
		ListResponse,
		sync_status::{AuditBridgeSummary, ReplicationPlan, SchemaCompatibilityReport},
	},
	AppState,
};

pub async fn run_federated_query(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(request): Json<FederatedQueryRequest>,
) -> ServiceResult<FederatedQueryResult> {
	let mut tx = open_scope(&state, &claims).await?;
	let shares = load_shares(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let grants = load_access_grants(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let peers = load_peers(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;

	let share = shares
		.iter()
		.find(|share| share.id == request.share_id)
		.cloned()
		.ok_or_else(|| not_found("shared dataset not found"))?;
	let grant = grants
		.iter()
		.find(|grant| grant.share_id == request.share_id)
		.cloned()
		.ok_or_else(|| not_found("access grant not found for shared dataset"))?;
	let peer_index = peers.into_iter().map(|peer| (peer.id, peer)).collect::<HashMap<_, _>>();

	let result = federation::execute_query(&request, &share, &grant, &peer_index)
		.map_err(internal_error)?;
	Ok(Json(result))
}

pub async fn list_replication_plans(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<ReplicationPlan>> {
	let mut tx = open_scope(&state, &claims).await?;
	let shares = load_shares(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let sync_statuses = load_sync_statuses(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let compatibility = shares.iter().map(schema_compat::evaluate).collect::<Vec<_>>();
	Ok(Json(ListResponse {
		items: replication::build_plans(&shares, &sync_statuses, &compatibility),
	}))
}

pub async fn list_schema_compatibility(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<SchemaCompatibilityReport>> {
	let mut tx = open_scope(&state, &claims).await?;
	let shares = load_shares(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse {
		items: shares.iter().map(schema_compat::evaluate).collect(),
	}))
}

pub async fn get_audit_bridge(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<AuditBridgeSummary> {
	let mut tx = open_scope(&state, &claims).await?;
	let peers = load_peers(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let contracts = load_contracts(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let shares = load_shares(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	let sync_statuses = load_sync_statuses(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(audit_bridge::summarize(&peers, &contracts, &shares, &sync_statuses)))
}
