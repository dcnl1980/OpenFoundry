use auth_middleware::layer::AuthUser;
use axum::{
	extract::{Path, State},
	Json,
};

use crate::{
	domain::tile_server,
	handlers::{db_error, internal_error, load_layer_row, not_found, scoped_tx, ServiceResult},
	models::{layer::LayerDefinition, spatial_index::VectorTileResponse},
	AppState,
};

pub async fn get_vector_tile(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<VectorTileResponse> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let row = load_layer_row(&mut tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("layer not found"))?;
	tx.commit().await.map_err(|cause| db_error(&cause))?;
	let layer = LayerDefinition::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;
	Ok(Json(tile_server::vector_tile(&layer)))
}
