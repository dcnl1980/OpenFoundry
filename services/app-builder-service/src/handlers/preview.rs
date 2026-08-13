use auth_middleware::layer::AuthUser;
use axum::{
	extract::{Path, State},
	Json,
};
use uuid::Uuid;

use crate::{
	domain::{embedding::build_embed_info, renderer},
	handlers::{db_error, load_app, load_published_app, scoped_tx, ServiceResult},
	models::app::{AppEmbedInfo, AppPreviewResponse, PublishedAppResponse},
	AppState,
};

pub async fn preview_app(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(id): Path<Uuid>,
) -> ServiceResult<Json<AppPreviewResponse>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let app = load_app(&mut tx, id).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(renderer::build_preview_response(
		app,
		&state.public_base_url,
	)))
}

pub async fn get_published_app(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(slug): Path<String>,
) -> ServiceResult<Json<PublishedAppResponse>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let (app, version) = load_published_app(&mut tx, &slug).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(renderer::build_published_response(
		app,
		version,
		&state.public_base_url,
	)))
}

pub async fn get_embed_info(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(slug): Path<String>,
) -> ServiceResult<Json<AppEmbedInfo>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let (app, _) = load_published_app(&mut tx, &slug).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(build_embed_info(&state.public_base_url, &app.slug)))
}
