use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;
use auth_middleware::layer::AuthUser;

use crate::{
	domain::{discovery, registry},
	handlers::{
		commit_scope, db_error, internal_error, load_listing_row, load_listings, load_reviews, load_versions, not_found,
		open_scope, ServiceResult,
	},
	models::{category::CategoryDefinition, listing::{ListingDefinition, ListingDetail, MarketplaceOverview, SearchResponse}, ListResponse},
	AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
	pub q: Option<String>,
	pub category: Option<String>,
}

pub async fn get_overview(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<MarketplaceOverview> {
	let mut tx = open_scope(&state, &claims).await?;
	let listings = load_listings(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let categories = discovery::featured_categories(&listings);
	let featured = listings.iter().take(3).cloned().collect();

	Ok(Json(MarketplaceOverview {
		listing_count: listings.len(),
		category_count: categories.len(),
		featured,
		total_installs: listings.iter().map(|listing| listing.install_count).sum(),
	}))
}

pub async fn list_categories(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<CategoryDefinition>> {
	let mut tx = open_scope(&state, &claims).await?;
	let listings = load_listings(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse {
		items: discovery::featured_categories(&listings),
	}))
}

pub async fn list_listings(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListResponse<ListingDefinition>> {
	let mut tx = open_scope(&state, &claims).await?;
	let listings = load_listings(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	Ok(Json(ListResponse { items: listings }))
}

pub async fn get_listing(
	Path(id): Path<uuid::Uuid>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<ListingDetail> {
	let mut tx = open_scope(&state, &claims).await?;
	let row = load_listing_row(&mut *tx, id)
		.await
		.map_err(|cause| db_error(&cause))?
		.ok_or_else(|| not_found("listing not found"))?;
	let listing = ListingDefinition::try_from(row).map_err(|cause| internal_error(cause.to_string()))?;
	let versions = load_versions(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	let reviews = load_reviews(&mut *tx, id).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let latest_version = registry::latest_version(&listing, &versions);
	Ok(Json(ListingDetail {
		listing,
		latest_version,
		versions,
		reviews,
	}))
}

pub async fn search_listings(
	Query(query): Query<SearchQuery>,
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> ServiceResult<SearchResponse> {
	let mut tx = open_scope(&state, &claims).await?;
	let listings = load_listings(&mut *tx).await.map_err(|cause| db_error(&cause))?;
	commit_scope(tx).await?;
	let query_text = query.q.unwrap_or_else(|| "widget".to_string());
	let results = listings
		.into_iter()
		.filter(|listing| query.category.as_ref().map(|category| category == &listing.category_slug).unwrap_or(true))
		.map(|listing| {
			let score = discovery::score_listing(&listing, &query_text);
			(listing, score)
		})
		.filter(|(_, score)| *score > 0.45)
		.collect::<Vec<_>>();

	Ok(Json(SearchResponse {
		query: query_text,
		results,
	}))
}
