use auth_middleware::layer::AuthUser;
use axum::{
	extract::{Path, State},
	Json,
};
use uuid::Uuid;

use crate::{
	handlers::{bad_request, db_error, load_app, persist_app, sanitize_pages, scoped_tx, ServiceResult},
	models::{app::App, page::AppPage},
	AppState,
};

pub async fn create_page(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path(app_id): Path<Uuid>,
	Json(page): Json<AppPage>,
) -> ServiceResult<Json<App>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let mut app = load_app(&mut tx, app_id).await?;
	app.pages.push(page);
	sanitize_pages(&mut app.pages, &mut app.settings);
	let app = persist_app(&mut tx, &app).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(app))
}

pub async fn update_page(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path((app_id, page_id)): Path<(Uuid, String)>,
	Json(mut page): Json<AppPage>,
) -> ServiceResult<Json<App>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let mut app = load_app(&mut tx, app_id).await?;
	let Some(index) = app.pages.iter().position(|candidate| candidate.id == page_id) else {
		return Err((axum::http::StatusCode::NOT_FOUND, "page not found".to_string()));
	};

	page.id = page_id;
	app.pages[index] = page;
	sanitize_pages(&mut app.pages, &mut app.settings);
	let app = persist_app(&mut tx, &app).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(app))
}

pub async fn delete_page(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Path((app_id, page_id)): Path<(Uuid, String)>,
) -> ServiceResult<Json<App>> {
	let mut tx = scoped_tx(&state, &claims).await?;
	let mut app = load_app(&mut tx, app_id).await?;
	if app.pages.len() <= 1 {
		return Err(bad_request("apps require at least one page"));
	}

	let previous_len = app.pages.len();
	app.pages.retain(|page| page.id != page_id);
	if app.pages.len() == previous_len {
		return Err((axum::http::StatusCode::NOT_FOUND, "page not found".to_string()));
	}

	sanitize_pages(&mut app.pages, &mut app.settings);
	let app = persist_app(&mut tx, &app).await?;
	tx.commit().await.map_err(db_error)?;
	Ok(Json(app))
}
