use axum::http::{
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName},
    HeaderValue, Method,
};
use tower_http::cors::CorsLayer;

const ALLOWED_HEADERS: [HeaderName; 4] = [
    AUTHORIZATION,
    CONTENT_TYPE,
    ACCEPT,
    HeaderName::from_static("x-request-id"),
];

pub fn cors_layer(origins: &[String]) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(ALLOWED_HEADERS)
        .max_age(std::time::Duration::from_secs(3600));

    let parsed_origins: Vec<HeaderValue> = origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    if parsed_origins.is_empty() {
        // Credentials cannot be combined with a wildcard origin.
        layer.allow_origin(tower_http::cors::Any)
    } else {
        layer.allow_credentials(true).allow_origin(parsed_origins)
    }
}

#[cfg(test)]
mod tests {
    use super::cors_layer;
    use tower::Layer;

    #[test]
    fn empty_origins_can_be_applied_as_a_layer() {
        let _ = cors_layer(&[]).layer(());
    }

    #[test]
    fn listed_origins_can_be_applied_as_a_layer() {
        let _ = cors_layer(&["http://127.0.0.1:4173".to_string()]).layer(());
    }
}
