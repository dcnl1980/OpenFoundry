use axum::{extract::State, Json};
use chrono::Utc;
use auth_middleware::layer::AuthUser;

use crate::{
	domain::cron,
	handlers::{db_error, internal_error, load_all_reports, load_execution_history, tenant::begin_scope, ServiceResult},
	models::snapshot::ScheduleBoard,
	AppState,
};

pub async fn get_schedule_board(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ScheduleBoard> {
	let mut tx = begin_scope(&state, &claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))?;
	let reports = load_all_reports(&mut tx).await.map_err(|cause| db_error(&cause))?;
	let recent_executions = load_execution_history(&mut tx, None, 5)
		.await
		.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|error| {
		tracing::error!("schedule board commit failed: {error}");
		internal_error("commit failed")
	})?;
	let board = cron::build_schedule_board(&reports, recent_executions, Utc::now());
	Ok(Json(board))
}
