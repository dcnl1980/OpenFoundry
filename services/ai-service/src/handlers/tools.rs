use axum::{
	extract::{Path, State},
	Json,
};
use serde_json::json;
use sqlx::{query_as, types::Json as SqlJson};
use uuid::Uuid;

use auth_middleware::layer::AuthUser;

use crate::{
	models::tool::{
		CreateToolRequest, ListToolsResponse, ToolDefinition, ToolRow, UpdateToolRequest,
	},
	AppState,
};

use super::{bad_request, db_error, internal_error, not_found, tenant::begin_scope, ServiceResult};

async fn load_tool_row(
	db: &mut sqlx::PgConnection,
	tool_id: Uuid,
) -> Result<Option<ToolRow>, sqlx::Error> {
	query_as::<_, ToolRow>(
		r#"
		SELECT
			id,
			name,
			description,
			category,
			execution_mode,
			status,
			input_schema,
			output_schema,
			tags,
			created_at,
			updated_at
		FROM ai_tools
		WHERE id = $1
		"#,
	)
	.bind(tool_id)
	.fetch_optional(&mut *db)
	.await
}

pub async fn list_tools(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListToolsResponse> {
	let mut tx = begin_scope(&state, &claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))?;
	let rows = query_as::<_, ToolRow>(
		r#"
		SELECT
			id,
			name,
			description,
			category,
			execution_mode,
			status,
			input_schema,
			output_schema,
			tags,
			created_at,
			updated_at
		FROM ai_tools
		ORDER BY updated_at DESC, created_at DESC
		"#,
	)
	.fetch_all(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|error| {
		tracing::error!("list tools commit failed: {error}");
		internal_error("commit failed")
	})?;

	Ok(Json(ListToolsResponse {
		data: rows.into_iter().map(Into::into).collect(),
	}))
}

pub async fn create_tool(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(body): Json<CreateToolRequest>,
) -> ServiceResult<ToolDefinition> {
	if body.name.trim().is_empty() {
		return Err(bad_request("tool name is required"));
	}

	let mut tx = begin_scope(&state, &claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))?;
	let tenant_id = claims.tenant_scope_id();
	let row = query_as::<_, ToolRow>(
		r#"
		INSERT INTO ai_tools (
			id,
			name,
			description,
			category,
			execution_mode,
			status,
			input_schema,
			output_schema,
			tags,
			tenant_id
		)
		VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
		RETURNING
			id,
			name,
			description,
			category,
			execution_mode,
			status,
			input_schema,
			output_schema,
			tags,
			created_at,
			updated_at
		"#,
	)
	.bind(Uuid::now_v7())
	.bind(body.name.trim())
	.bind(body.description)
	.bind(body.category)
	.bind(body.execution_mode)
	.bind(body.status)
	.bind(SqlJson(if body.input_schema.is_null() {
		json!({})
	} else {
		body.input_schema
	}))
	.bind(SqlJson(if body.output_schema.is_null() {
		json!({})
	} else {
		body.output_schema
	}))
	.bind(SqlJson(body.tags))
	.bind(tenant_id)
	.fetch_one(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|error| {
		tracing::error!("create tool commit failed: {error}");
		internal_error("commit failed")
	})?;

	Ok(Json(row.into()))
}

pub async fn update_tool(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(tool_id): Path<Uuid>,
	Json(body): Json<UpdateToolRequest>,
) -> ServiceResult<ToolDefinition> {
	let mut tx = begin_scope(&state, &claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))?;
	let Some(current) = load_tool_row(&mut tx, tool_id)
		.await
		.map_err(|cause| db_error(&cause))?
	else {
		return Err(not_found("tool not found"));
	};

	let tool: ToolDefinition = current.into();
	let row = query_as::<_, ToolRow>(
		r#"
		UPDATE ai_tools
		SET name = $2,
			description = $3,
			category = $4,
			execution_mode = $5,
			status = $6,
			input_schema = $7,
			output_schema = $8,
			tags = $9,
			updated_at = NOW()
		WHERE id = $1
		RETURNING
			id,
			name,
			description,
			category,
			execution_mode,
			status,
			input_schema,
			output_schema,
			tags,
			created_at,
			updated_at
		"#,
	)
	.bind(tool_id)
	.bind(body.name.unwrap_or(tool.name))
	.bind(body.description.unwrap_or(tool.description))
	.bind(body.category.unwrap_or(tool.category))
	.bind(body.execution_mode.unwrap_or(tool.execution_mode))
	.bind(body.status.unwrap_or(tool.status))
	.bind(SqlJson(body.input_schema.unwrap_or(tool.input_schema)))
	.bind(SqlJson(body.output_schema.unwrap_or(tool.output_schema)))
	.bind(SqlJson(body.tags.unwrap_or(tool.tags)))
	.fetch_one(&mut *tx)
	.await
	.map_err(|cause| db_error(&cause))?;
	tx.commit().await.map_err(|error| {
		tracing::error!("update tool commit failed: {error}");
		internal_error("commit failed")
	})?;

	Ok(Json(row.into()))
}
