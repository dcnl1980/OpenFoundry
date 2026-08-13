use axum::{
	extract::State,
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use serde_json::json;
use auth_middleware::layer::AuthUser;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
	handlers::tenant::begin_scope,
	models::subscription::{NotificationPreference, UpdateNotificationPreferenceRequest},
	AppState,
};

pub async fn get_preferences(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	match load_or_default_preferences(&mut tx, claims.sub, claims.tenant_scope_id()).await {
		Ok(preferences) => {
			if let Err(error) = tx.commit().await {
				tracing::error!("get notification preferences commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			Json(preferences).into_response()
		}
		Err(error) => {
			tracing::error!("get notification preferences failed: {error}");
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub async fn update_preferences(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Json(body): Json<UpdateNotificationPreferenceRequest>,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	let current = match load_or_default_preferences(&mut tx, claims.sub, claims.tenant_scope_id()).await {
		Ok(preferences) => preferences,
		Err(error) => {
			tracing::error!("load current notification preferences failed: {error}");
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	};
	let tenant_id = claims.tenant_scope_id();

	let updated = sqlx::query_as::<_, NotificationPreference>(
		r#"INSERT INTO notification_preferences (
			   user_id, in_app_enabled, email_enabled, email_address, slack_webhook_url, teams_webhook_url, digest_frequency, quiet_hours, tenant_id
		   )
		   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
		   ON CONFLICT (user_id)
		   DO UPDATE SET
			   in_app_enabled = EXCLUDED.in_app_enabled,
			   email_enabled = EXCLUDED.email_enabled,
			   email_address = EXCLUDED.email_address,
			   slack_webhook_url = EXCLUDED.slack_webhook_url,
			   teams_webhook_url = EXCLUDED.teams_webhook_url,
			   digest_frequency = EXCLUDED.digest_frequency,
			   quiet_hours = EXCLUDED.quiet_hours,
			   tenant_id = EXCLUDED.tenant_id,
			   updated_at = NOW()
		   RETURNING *"#,
	)
	.bind(claims.sub)
	.bind(body.in_app_enabled.unwrap_or(current.in_app_enabled))
	.bind(body.email_enabled.unwrap_or(current.email_enabled))
	.bind(body.email_address.or(current.email_address))
	.bind(body.slack_webhook_url.or(current.slack_webhook_url))
	.bind(body.teams_webhook_url.or(current.teams_webhook_url))
	.bind(body.digest_frequency.unwrap_or(current.digest_frequency))
	.bind(body.quiet_hours.unwrap_or(current.quiet_hours))
	.bind(tenant_id)
	.fetch_one(&mut *tx)
	.await;

	match updated {
		Ok(preferences) => {
			if let Err(error) = tx.commit().await {
				tracing::error!("update notification preferences commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			Json(preferences).into_response()
		}
		Err(error) => {
			tracing::error!("update notification preferences failed: {error}");
			(StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": error.to_string() }))).into_response()
		}
	}
}

pub async fn load_or_default_preferences(
	tx: &mut Transaction<'_, Postgres>,
	user_id: Uuid,
	tenant_id: Uuid,
) -> Result<NotificationPreference, sqlx::Error> {
	let existing = sqlx::query_as::<_, NotificationPreference>(
		r#"SELECT * FROM notification_preferences WHERE user_id = $1"#,
	)
	.bind(user_id)
	.fetch_optional(&mut **tx)
	.await?;

	if let Some(existing) = existing {
		Ok(existing)
	} else {
		Ok(NotificationPreference {
			user_id,
			in_app_enabled: true,
			email_enabled: false,
			email_address: None,
			slack_webhook_url: None,
			teams_webhook_url: None,
			digest_frequency: "instant".to_string(),
			quiet_hours: json!({}),
			updated_at: chrono::Utc::now(),
			tenant_id,
		})
	}
}
