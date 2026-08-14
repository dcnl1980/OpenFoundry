use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::domain::rbac;
use crate::handlers::tenant::begin_tenant_id;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> impl IntoResponse {
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM openfoundry_lookup_user_by_email($1)",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await;

    if matches!(existing, Ok(Some(_))) {
        return (
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "email already registered" })),
        )
            .into_response();
    }

    let password_hash = match hash_password(&body.password) {
        Ok(h) => h,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to hash password" })),
            )
                .into_response()
        }
    };

    let user_id = Uuid::now_v7();
    let tenant_id = user_id;
    let mut tx = match begin_tenant_id(&state, tenant_id).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };

    let result = sqlx::query(
        r#"INSERT INTO users (id, email, name, password_hash, is_active, auth_source, tenant_id)
              VALUES ($1, $2, $3, $4, true, 'local', $5)"#,
    )
    .bind(user_id)
    .bind(&body.email)
    .bind(&body.name)
    .bind(&password_hash)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await;

    if result.is_ok() {
        if let Err(e) = rbac::assign_founding_role(
            &mut tx,
            user_id,
            tenant_id,
            &body.email,
            state.bootstrap_admin_email.as_deref(),
        )
        .await
        {
            tracing::error!("failed to assign founding role: {e}");
        }
    }

    match result {
        Ok(_) => {
            if let Err(e) = tx.commit().await {
                tracing::error!("registration commit failed: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": "registration failed" })),
                )
                    .into_response();
            }

            tracing::info!(user_id = %user_id, email = %body.email, "user registered");

            (
                StatusCode::CREATED,
                Json(serde_json::json!(RegisterResponse {
                    id: user_id,
                    email: body.email,
                    name: body.name,
                })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("registration failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "registration failed" })),
            )
                .into_response()
        }
    }
}

fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::{Argon2, PasswordHasher};
    use argon2::password_hash::SaltString;
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}
