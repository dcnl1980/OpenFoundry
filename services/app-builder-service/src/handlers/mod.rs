pub mod apps;
pub mod pages;
pub mod preview;
pub mod publish;
pub mod tenant;
pub mod widgets;

use auth_middleware::Claims;
use axum::http::StatusCode;
use sqlx::{types::Json, Postgres, Transaction};
use uuid::Uuid;

use crate::{
	models::{
		app::{App, AppRow, AppSettings, AppTemplate, AppTemplateRow},
		page::AppPage,
		version::{AppVersion, AppVersionRow},
		widget::WidgetDefinition,
	},
	AppState,
};

pub type ServiceResult<T> = Result<T, (StatusCode, String)>;

pub fn bad_request(message: impl Into<String>) -> (StatusCode, String) {
	(StatusCode::BAD_REQUEST, message.into())
}

pub fn conflict(message: impl Into<String>) -> (StatusCode, String) {
	(StatusCode::CONFLICT, message.into())
}

pub fn not_found(resource: &str) -> (StatusCode, String) {
	(StatusCode::NOT_FOUND, format!("{resource} not found"))
}

pub fn internal_error(message: impl Into<String>) -> (StatusCode, String) {
	(StatusCode::INTERNAL_SERVER_ERROR, message.into())
}

pub fn db_error(error: sqlx::Error) -> (StatusCode, String) {
	if let Some(database_error) = error.as_database_error() {
		if let Some(constraint) = database_error.constraint() {
			if constraint == "apps_slug_key" || constraint == "apps_tenant_slug_key" {
				return conflict("app slug already exists");
			}
		}
	}

	tracing::error!("app-builder database error: {error}");
	internal_error("database operation failed")
}

pub fn slugify(input: &str) -> String {
	let mut slug = String::new();
	let mut last_dash = false;

	for character in input.chars().flat_map(char::to_lowercase) {
		if character.is_ascii_alphanumeric() {
			slug.push(character);
			last_dash = false;
		} else if !last_dash {
			slug.push('-');
			last_dash = true;
		}
	}

	let trimmed = slug.trim_matches('-').to_string();
	if trimmed.is_empty() {
		"app".to_string()
	} else {
		trimmed
	}
}

pub fn normalize_slug(candidate: Option<&str>, fallback_name: &str) -> String {
	let raw = candidate.unwrap_or(fallback_name).trim();
	slugify(raw)
}

pub fn sanitize_pages(pages: &mut Vec<AppPage>, settings: &mut AppSettings) {
	if pages.is_empty() {
		pages.push(AppPage::default());
	}

	for (index, page) in pages.iter_mut().enumerate() {
		if page.id.trim().is_empty() {
			page.id = Uuid::now_v7().to_string();
		}

		if page.name.trim().is_empty() {
			page.name = format!("Page {}", index + 1);
		}

		if page.path.trim().is_empty() {
			page.path = if index == 0 {
				"/".to_string()
			} else {
				format!("/{}", slugify(&page.name))
			};
		}

		sanitize_widgets(&mut page.widgets);
	}

	let current_home_exists = settings
		.home_page_id
		.as_ref()
		.map(|page_id| pages.iter().any(|page| &page.id == page_id))
		.unwrap_or(false);

	if !current_home_exists {
		settings.home_page_id = pages.first().map(|page| page.id.clone());
	}
}

fn sanitize_widgets(widgets: &mut [WidgetDefinition]) {
	for widget in widgets {
		if widget.id.trim().is_empty() {
			widget.id = Uuid::now_v7().to_string();
		}

		if widget.title.trim().is_empty() {
			widget.title = "Untitled widget".to_string();
		}

		sanitize_widgets(&mut widget.children);
	}
}

pub async fn scoped_tx<'a>(
	state: &'a AppState,
	claims: &Claims,
) -> ServiceResult<Transaction<'a, Postgres>> {
	tenant::begin_scope(state, claims)
		.await
		.map_err(|_| internal_error("tenant scope failed"))
}

pub async fn load_app(
	tx: &mut Transaction<'_, Postgres>,
	app_id: Uuid,
) -> ServiceResult<App> {
	let row = sqlx::query_as::<_, AppRow>("SELECT * FROM apps WHERE id = $1")
		.bind(app_id)
		.fetch_optional(&mut **tx)
		.await
		.map_err(db_error)?;

	row.map(Into::into).ok_or_else(|| not_found("app"))
}

pub async fn load_template_by_key(
	tx: &mut Transaction<'_, Postgres>,
	key: &str,
) -> ServiceResult<AppTemplate> {
	let row = sqlx::query_as::<_, AppTemplateRow>(
		"SELECT id, key, name, description, category, preview_image_url, definition, created_at
		 FROM app_templates
		 WHERE key = $1",
	)
	.bind(key)
	.fetch_optional(&mut **tx)
	.await
	.map_err(db_error)?;

	row.map(Into::into)
		.ok_or_else(|| not_found("app template"))
}

pub async fn load_published_app(
	tx: &mut Transaction<'_, Postgres>,
	slug: &str,
) -> ServiceResult<(App, AppVersion)> {
	let app_row = sqlx::query_as::<_, AppRow>(
		"SELECT * FROM apps WHERE slug = $1 AND published_version_id IS NOT NULL",
	)
	.bind(slug)
	.fetch_optional(&mut **tx)
	.await
	.map_err(db_error)?;

	let app: App = app_row.map(Into::into).ok_or_else(|| not_found("published app"))?;
	let version_id = app
		.published_version_id
		.ok_or_else(|| not_found("published app version"))?;

	let version_row = sqlx::query_as::<_, AppVersionRow>(
		"SELECT * FROM app_versions WHERE id = $1",
	)
	.bind(version_id)
	.fetch_optional(&mut **tx)
	.await
	.map_err(db_error)?;

	let version = version_row
		.map(Into::into)
		.ok_or_else(|| not_found("published app version"))?;

	Ok((app, version))
}

pub async fn persist_app(
	tx: &mut Transaction<'_, Postgres>,
	app: &App,
) -> ServiceResult<App> {
	let row = sqlx::query_as::<_, AppRow>(
		"UPDATE apps
		 SET name = $2,
			 slug = $3,
			 description = $4,
			 status = $5,
			 pages = $6,
			 theme = $7,
			 settings = $8,
			 template_key = $9,
			 published_version_id = $10,
			 updated_at = NOW()
		 WHERE id = $1
		 RETURNING *",
	)
	.bind(app.id)
	.bind(&app.name)
	.bind(&app.slug)
	.bind(&app.description)
	.bind(&app.status)
	.bind(Json(app.pages.clone()))
	.bind(Json(app.theme.clone()))
	.bind(Json(app.settings.clone()))
	.bind(&app.template_key)
	.bind(app.published_version_id)
	.fetch_one(&mut **tx)
	.await
	.map_err(db_error)?;

	Ok(row.into())
}