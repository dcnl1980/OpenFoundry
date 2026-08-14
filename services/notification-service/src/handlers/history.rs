use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use serde_json::json;
use uuid::Uuid;
use auth_middleware::layer::AuthUser;

use crate::{
	handlers::{send::unread_count, tenant::begin_scope},
	models::notification::{ListNotificationsQuery, NotificationEvent, NotificationRecord},
	AppState,
};

pub async fn list_notifications(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
	Query(params): Query<ListNotificationsQuery>,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	let limit = params.limit.unwrap_or(20).clamp(1, 100);

	let notifications = sqlx::query_as::<_, NotificationRecord>(
		r#"SELECT * FROM notifications
		   WHERE (user_id = $1 OR user_id IS NULL)
			 AND ($2::TEXT IS NULL OR status = $2)
		   ORDER BY created_at DESC
		   LIMIT $3"#,
	)
	.bind(claims.sub)
	.bind(&params.status)
	.bind(limit)
	.fetch_all(&mut *tx)
	.await;

	match notifications {
		Ok(data) => {
			let unread = unread_count(&mut tx, Some(claims.sub)).await.unwrap_or(0);
			if let Err(error) = tx.commit().await {
				tracing::error!("list notifications commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			Json(json!({
				"data": data,
				"unread_count": unread,
			}))
			.into_response()
		}
		Err(error) => {
			tracing::error!("list notifications failed: {error}");
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub async fn mark_read(
	State(state): State<AppState>,
	Path(notification_id): Path<Uuid>,
	AuthUser(claims): AuthUser,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	let notification = sqlx::query_as::<_, NotificationRecord>(
		r#"UPDATE notifications
		   SET status = 'read', read_at = NOW()
		   WHERE id = $1 AND (user_id = $2 OR user_id IS NULL)
		   RETURNING *"#,
	)
	.bind(notification_id)
	.bind(claims.sub)
	.fetch_optional(&mut *tx)
	.await;

	match notification {
		Ok(Some(notification)) => {
			let unread = unread_count(&mut tx, Some(claims.sub)).await.unwrap_or(0);
			if let Err(error) = tx.commit().await {
				tracing::error!("mark notification read commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			let _ = state.notification_bus.send(NotificationEvent {
				kind: "notification.read".to_string(),
				user_id: Some(claims.sub),
				notification: Some(notification.clone()),
				unread_count: unread,
			});
			Json(json!({ "notification": notification, "unread_count": unread })).into_response()
		}
		Ok(None) => StatusCode::NOT_FOUND.into_response(),
		Err(error) => {
			tracing::error!("mark notification read failed: {error}");
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub async fn mark_all_read(
	State(state): State<AppState>,
	AuthUser(claims): AuthUser,
) -> impl IntoResponse {
	let mut tx = match begin_scope(&state, &claims).await {
		Ok(tx) => tx,
		Err(response) => return response,
	};
	match sqlx::query(
		r#"UPDATE notifications
		   SET status = 'read', read_at = NOW()
		   WHERE status = 'unread' AND (user_id = $1 OR user_id IS NULL)"#,
	)
	.bind(claims.sub)
	.execute(&mut *tx)
	.await
	{
		Ok(_) => {
			if let Err(error) = tx.commit().await {
				tracing::error!("mark all notifications read commit failed: {error}");
				return StatusCode::INTERNAL_SERVER_ERROR.into_response();
			}
			let _ = state.notification_bus.send(NotificationEvent {
				kind: "notification.read_all".to_string(),
				user_id: Some(claims.sub),
				notification: None,
				unread_count: 0,
			});
			Json(json!({ "unread_count": 0 })).into_response()
		}
		Err(error) => {
			tracing::error!("mark all notifications read failed: {error}");
			StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}
