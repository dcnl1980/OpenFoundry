use axum::{
    extract::{Query, State, WebSocketUpgrade},
    http::StatusCode,
    response::IntoResponse,
};
use axum::extract::ws::{Message, WebSocket};
use auth_middleware::begin_tenant_transaction;

use crate::{
    handlers::send::{latest_notifications, unread_count},
    models::notification::{NotificationEvent, WebSocketQuery},
    AppState,
};

pub async fn notifications_ws(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(params): Query<WebSocketQuery>,
) -> impl IntoResponse {
    let claims = match auth_middleware::jwt::decode_token(&state.jwt_config, &params.token) {
        Ok(claims) => claims,
        Err(_) => return StatusCode::UNAUTHORIZED.into_response(),
    };

    ws.on_upgrade(move |socket| websocket_loop(socket, state, claims))
}

async fn websocket_loop(
    mut socket: WebSocket,
    state: AppState,
    claims: auth_middleware::Claims,
) {
    let user_id = claims.sub;
    let notifications = match begin_tenant_transaction(&state.db, claims.tenant_scope_id()).await {
        Ok(mut tx) => {
            let notifications = latest_notifications(&mut tx, user_id, 20)
                .await
                .unwrap_or_default();
            let unread = unread_count(&mut tx, Some(user_id)).await.unwrap_or(0);
            let _ = tx.commit().await;
            (notifications, unread)
        }
        Err(_) => (Vec::new(), 0),
    };
    let snapshot = serde_json::json!({
        "kind": "snapshot",
        "data": notifications.0,
        "unread_count": notifications.1,
    });

    if socket
        .send(Message::Text(snapshot.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let mut rx = state.notification_bus.subscribe();
    while let Ok(event) = rx.recv().await {
        if !targets_user(&event, user_id) {
            continue;
        }

        let payload = match serde_json::to_string(&event) {
            Ok(payload) => payload,
            Err(_) => continue,
        };

        if socket.send(Message::Text(payload.into())).await.is_err() {
            break;
        }
    }
}

fn targets_user(event: &NotificationEvent, user_id: uuid::Uuid) -> bool {
    event.user_id.map(|target| target == user_id).unwrap_or(true)
}
