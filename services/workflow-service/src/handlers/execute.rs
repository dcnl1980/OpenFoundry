use axum::{
	extract::{Path, State},
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
	domain::executor,
	handlers::crud::load_workflow,
	models::{execution::StartRunRequest, execution::TriggerEventRequest, workflow::WorkflowDefinition},
	AppState,
};
use auth_middleware::{begin_tenant_transaction, layer::AuthUser, DueWork};

use super::tenant::begin_scope;

pub async fn start_manual_run(
	State(state): State<AppState>,
	Path(workflow_id): Path<Uuid>,
	AuthUser(claims): AuthUser,
	Json(body): Json<StartRunRequest>,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	let Some(workflow) = (match load_workflow(&mut tx, workflow_id).await {
		Ok(workflow) => workflow,
		Err(error) => {
			tracing::error!("manual run lookup failed: {error}");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	}) else {
		return StatusCode::NOT_FOUND.into_response();
	};
	if let Err(error) = tx.commit().await {
		tracing::error!("manual run lookup commit failed: {error}");
		return StatusCode::INTERNAL_SERVER_ERROR.into_response();
	}

	match executor::execute_workflow_run(
		&state,
		&workflow,
		"manual",
		Some(claims.sub),
		body.context,
	)
	.await
	{
		Ok(run) => (StatusCode::CREATED, Json(run)).into_response(),
		Err(error) => (
			StatusCode::BAD_REQUEST,
			Json(json!({ "error": error })),
		)
			.into_response(),
	}
}

pub async fn trigger_event(
	State(state): State<AppState>,
	Path(event_name): Path<String>,
	AuthUser(claims): AuthUser,
	Json(body): Json<TriggerEventRequest>,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	let workflows = sqlx::query_as::<_, WorkflowDefinition>(
		r#"SELECT * FROM workflows
		   WHERE status = 'active'
			 AND trigger_type = 'event'
			 AND trigger_config ->> 'event_name' = $1
		   ORDER BY updated_at DESC"#,
	)
	.bind(&event_name)
	.fetch_all(&mut *tx)
	.await;

	match workflows {
		Ok(workflows) => {
			if let Err(error) = tx.commit().await {
				tracing::error!("event workflow lookup commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			let mut triggered = Vec::new();
			for workflow in workflows {
				let payload = json!({
					"trigger": {
						"type": "event",
						"name": event_name,
						"actor_id": claims.sub,
					},
					"event": body.context,
				});

				match executor::execute_workflow_run(
					&state,
					&workflow,
					"event",
					Some(claims.sub),
					payload,
				)
				.await
				{
					Ok(run) => triggered.push(run),
					Err(error) => tracing::warn!(workflow_id = %workflow.id, "event trigger failed: {error}"),
				}
			}

			Json(json!({ "data": triggered, "event_name": event_name })).into_response()
		}
		Err(error) => {
			tracing::error!("event workflow lookup failed: {error}");
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub async fn trigger_webhook(
	State(state): State<AppState>,
	Path(workflow_id): Path<Uuid>,
	headers: HeaderMap,
	Json(body): Json<TriggerEventRequest>,
) -> impl IntoResponse {
	let resolved = sqlx::query_as::<_, DueWork>(
		"SELECT id, tenant_id FROM openfoundry_webhook_workflow($1)",
	)
	.bind(workflow_id)
	.fetch_optional(&state.db)
	.await;

	let Some(resolved) = (match resolved {
		Ok(row) => row,
		Err(error) => {
			tracing::error!("webhook tenant lookup failed: {error}");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	}) else {
		return StatusCode::NOT_FOUND.into_response();
	};

	let mut tx = match begin_tenant_transaction(&state.db, resolved.tenant_id).await {
		Ok(tx) => tx,
		Err(error) => {
			tracing::error!("webhook tenant transaction failed: {error}");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	};

	let Some(workflow) = (match load_workflow(&mut tx, workflow_id).await {
		Ok(workflow) => workflow,
		Err(error) => {
			tracing::error!("webhook lookup failed: {error}");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	}) else {
		return StatusCode::NOT_FOUND.into_response();
	};

	if workflow.trigger_type != "webhook" {
		return (
			StatusCode::BAD_REQUEST,
			Json(json!({ "error": "workflow is not configured for webhook triggers" })),
		)
			.into_response();
	}

	if let Some(expected_secret) = workflow.webhook_secret.as_deref() {
		let actual = headers
			.get("x-openfoundry-webhook-secret")
			.and_then(|value| value.to_str().ok())
			.unwrap_or_default();
		if actual != expected_secret {
			return StatusCode::UNAUTHORIZED.into_response();
		}
	}
	if let Err(error) = tx.commit().await {
		tracing::error!("webhook lookup commit failed: {error}");
		return StatusCode::INTERNAL_SERVER_ERROR.into_response();
	}

	match executor::execute_workflow_run(
		&state,
		&workflow,
		"webhook",
		None,
		json!({
			"trigger": {
				"type": "webhook",
				"workflow_id": workflow_id,
			},
			"payload": body.context,
		}),
	)
	.await
	{
		Ok(run) => (StatusCode::CREATED, Json(run)).into_response(),
		Err(error) => (
			StatusCode::BAD_REQUEST,
			Json(json!({ "error": error })),
		)
			.into_response(),
	}
}

pub async fn run_due_cron_workflows(
	_user: AuthUser,
	State(state): State<AppState>,
) -> impl IntoResponse {
	match executor::run_due_cron_workflows(&state).await {
		Ok(triggered_runs) => Json(json!({ "triggered_runs": triggered_runs })).into_response(),
		Err(error) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(json!({ "error": error })),
		)
			.into_response(),
	}
}
