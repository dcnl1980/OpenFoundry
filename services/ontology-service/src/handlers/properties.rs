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

pub async fn list_properties(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(type_id): Path<Uuid>,
) -> impl IntoResponse {
    match sqlx::query_as::<_, Property>(
        r#"SELECT * FROM properties WHERE object_type_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(type_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(properties) => Json(properties).into_response(),
        Err(e) => {
            tracing::error!("list properties: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn create_property(
    _user: AuthUser,
    State(state): State<AppState>,
    Path(type_id): Path<Uuid>,
    Json(body): Json<CreatePropertyRequest>,
) -> impl IntoResponse {
    let prepared = match prepare_new_property(&body) {
        Ok(prepared) => prepared,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };

    let object_type_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM object_types WHERE id = $1)",
    )
    .bind(type_id)
    .fetch_one(&state.db)
    .await;

    match object_type_exists {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("create property type lookup: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    }

    let id = Uuid::now_v7();
    let result = sqlx::query_as::<_, Property>(
        r#"INSERT INTO properties (
               id, object_type_id, name, display_name, description, property_type,
               required, unique_constraint, default_value, validation_rules
           )
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
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
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(property) => (StatusCode::CREATED, Json(property)).into_response(),
        Err(e) => {
            tracing::error!("create property: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
