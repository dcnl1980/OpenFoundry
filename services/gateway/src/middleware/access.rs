use axum::{
	extract::Request,
	http::{header::AUTHORIZATION, Method, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
	Json,
};
use auth_middleware::{jwt, JwtConfig};
use serde_json::{json, Value};

#[derive(Clone)]
pub struct AccessState {
	pub jwt_secret: String,
}

pub(crate) fn is_public_path(method: &Method, path: &str) -> bool {
	if path == "/health" {
		return *method == Method::GET || *method == Method::HEAD;
	}

	match (method, path) {
		(&Method::POST, "/api/v1/auth/register")
		| (&Method::POST, "/api/v1/auth/login")
		| (&Method::POST, "/api/v1/auth/refresh")
		| (&Method::POST, "/api/v1/auth/refresh-token")
		| (&Method::POST, "/api/v1/auth/mfa/complete")
		| (&Method::GET, "/api/v1/auth/sso/providers/public")
		| (&Method::POST, "/api/v1/auth/sso/callback") => true,
		(&Method::GET, path) => is_sso_start_path(path),
		_ => false,
	}
}

fn is_sso_start_path(path: &str) -> bool {
	path.strip_prefix("/api/v1/auth/sso/providers/")
		.and_then(|rest| rest.strip_suffix("/start"))
		.is_some_and(|slug| {
			!slug.is_empty() && slug != "public" && !slug.contains('/') && !slug.contains("..")
		})
}

pub(crate) fn is_access_token(token_use: Option<&str>) -> bool {
	matches!(token_use, None | Some("access") | Some("api_key"))
}

pub async fn require_auth_layer(
	axum::extract::State(state): axum::extract::State<AccessState>,
	req: Request,
	next: Next,
) -> Response {
	if is_public_path(req.method(), req.uri().path()) {
		return next.run(req).await;
	}

	let allowed = req
		.headers()
		.get(AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.and_then(|token| jwt::decode_token(&JwtConfig::new(&state.jwt_secret), token).ok())
		.is_some_and(|claims| is_access_token(claims.token_use.as_deref()));

	if allowed {
		next.run(req).await
	} else {
		(StatusCode::UNAUTHORIZED, Json(unauthorized_payload())).into_response()
	}
}

pub(crate) fn unauthorized_payload() -> Value {
	json!({ "error": "unauthorized" })
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn health_and_auth_entry_points_are_public() {
		assert!(is_public_path(&Method::GET, "/health"));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/login"));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/register"));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/refresh"));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/refresh-token"));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/mfa/complete"));
		assert!(is_public_path(
			&Method::GET,
			"/api/v1/auth/sso/providers/public"
		));
		assert!(is_public_path(
			&Method::GET,
			"/api/v1/auth/sso/providers/acme/start"
		));
		assert!(is_public_path(&Method::POST, "/api/v1/auth/sso/callback"));
	}

	#[test]
	fn api_and_wrong_method_auth_routes_are_private() {
		assert!(!is_public_path(&Method::GET, "/api/v1/datasets"));
		assert!(!is_public_path(&Method::GET, "/api/v1/auth/login"));
		assert!(!is_public_path(&Method::POST, "/api/v1/users/me"));
		assert!(!is_public_path(
			&Method::GET,
			"/api/v1/auth/sso/providers/acme/../admin/start"
		));
		assert!(!is_public_path(
			&Method::GET,
			"/api/v1/auth/sso/providers/public/start"
		));
	}

	#[test]
	fn only_access_and_api_key_tokens_are_accepted() {
		assert!(is_access_token(Some("access")));
		assert!(is_access_token(Some("api_key")));
		assert!(is_access_token(None));
		assert!(!is_access_token(Some("refresh")));
		assert!(!is_access_token(Some("id")));
	}

	#[test]
	fn unauthorized_payload_is_json_error() {
		assert_eq!(unauthorized_payload()["error"], "unauthorized");
	}
}
