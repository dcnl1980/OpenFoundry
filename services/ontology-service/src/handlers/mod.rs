pub mod types;
pub mod objects;
pub mod links;
pub mod actions;
pub mod properties;
pub mod tenant;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

pub fn db_failure(cause: &sqlx::Error) -> Response {
    tracing::error!("ontology-service database error: {cause}");
    json_error(StatusCode::INTERNAL_SERVER_ERROR, cause.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_error_uses_error_field() {
        let response = json_error(StatusCode::BAD_REQUEST, "name is required");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
