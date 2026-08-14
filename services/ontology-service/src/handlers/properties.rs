use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::domain::type_system::prepare_new_property;
use crate::models::property::{CreatePropertyRequest, Property};
use crate::AppState;
use auth_middleware::layer::AuthUser;

use super::tenant::begin_scope;

pub async fn list_properties(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(type_id): Path<Uuid>,
) -> impl IntoResponse {
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };
    match sqlx::query_as::<_, Property>(
        r#"SELECT * FROM properties WHERE object_type_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(type_id)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(properties) => {
            if let Err(error) = tx.commit().await {
                return super::db_failure(&error);
            }
            Json(properties).into_response()
        }
        Err(e) => {
            tracing::error!("list properties: {e}");
            super::db_failure(&e)
        }
    }
}

pub async fn create_property(
    AuthUser(claims): AuthUser,
    State(state): State<AppState>,
    Path(type_id): Path<Uuid>,
    Json(body): Json<CreatePropertyRequest>,
) -> impl IntoResponse {
    let prepared = match prepare_new_property(&body) {
        Ok(prepared) => prepared,
        Err(message) => return super::json_error(StatusCode::BAD_REQUEST, message),
    };
    let mut tx = match begin_scope(&state, &claims).await {
        Ok(tx) => tx,
        Err(response) => return response,
    };

    let object_type_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM object_types WHERE id = $1)",
    )
    .bind(type_id)
    .fetch_one(&mut *tx)
    .await;

    match object_type_exists {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("create property type lookup: {e}");
            return super::db_failure(&e);
        }
    }

    let id = Uuid::now_v7();
    let result = sqlx::query_as::<_, Property>(
        r#"INSERT INTO properties (
               id, object_type_id, name, display_name, description, property_type,
               required, unique_constraint, default_value, validation_rules, tenant_id
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
           RETURNING *"#,
    )
    .bind(id)
    .bind(type_id)
    .bind(&prepared.name)
    .bind(&prepared.display_name)
    .bind(&prepared.description)
    .bind(&prepared.property_type)
    .bind(prepared.required)
    .bind(prepared.unique_constraint)
    .bind(&prepared.default_value)
    .bind(&prepared.validation_rules)
    .bind(claims.tenant_scope_id())
    .fetch_one(&mut *tx)
    .await;

    match result {
        Ok(property) => {
            if let Err(error) = tx.commit().await {
                return super::db_failure(&error);
            }
            (StatusCode::CREATED, Json(property)).into_response()
        }
        Err(e) => {
            tracing::error!("create property: {e}");
            super::db_failure(&e)
        }
    }
}
