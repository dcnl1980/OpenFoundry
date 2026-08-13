use axum::{extract::{Path, State}, Json};
use auth_middleware::layer::AuthUser;

use crate::{
	handlers::{db_error, internal_error, load_execution_row, not_found, tenant::begin_scope, ServiceResult},
	models::snapshot::{DownloadPayload, ReportExecution},
	AppState,
};

pub async fn download_execution(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<DownloadPayload> {
	let mut tx = begin_scope(&state, &claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))?;
	let row = load_execution_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("report execution not found"))?;
	tx.commit().await.map_err(|error| {
		tracing::error!("download execution commit failed: {error}");
		internal_error("commit failed")
	})?;
	let execution = ReportExecution::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;

	Ok(Json(DownloadPayload {
		file_name: execution.artifact.file_name.clone(),
		mime_type: execution.artifact.mime_type.clone(),
		storage_url: execution.artifact.storage_url.clone(),
		preview_excerpt: execution.preview.headline.clone(),
		report_name: execution.report_name,
	}))
}
