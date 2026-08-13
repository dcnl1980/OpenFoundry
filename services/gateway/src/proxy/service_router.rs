use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::{Request, State},
    http::{
        header::{HeaderName, HeaderValue, AUTHORIZATION},
        HeaderMap, StatusCode, Uri,
    },
    response::{IntoResponse, Response},
};
use auth_middleware::{jwt, tenant::TenantContext, JwtConfig};
use futures::StreamExt;
use reqwest::Client;

use crate::config::GatewayConfig;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
];

fn hop_by_hop_names(headers: &HeaderMap) -> HashSet<String> {
    let mut names: HashSet<String> = HOP_BY_HOP.iter().map(|name| (*name).to_string()).collect();
    if let Some(connection) = headers.get("connection").and_then(|value| value.to_str().ok()) {
        for token in connection.split(',') {
            let name = token.trim().to_ascii_lowercase();
            if !name.is_empty() {
                names.insert(name);
            }
        }
    }
    names
}

fn insert_trusted_header(headers: &mut HeaderMap, name: &'static str, value: &str) {
    let Ok(name) = HeaderName::try_from(name) else {
        return;
    };
    if let Ok(value) = HeaderValue::from_str(value) {
        headers.insert(name, value);
    }
}

/// Drop client-supplied tenant headers and hop-by-hop headers before proxying.
pub(crate) fn sanitize_forward_headers(headers: &HeaderMap) -> HeaderMap {
    let hop_by_hop = hop_by_hop_names(headers);
    let mut forwarded = HeaderMap::new();
    for (key, value) in headers.iter() {
        let name = key.as_str();
        if name.starts_with("x-openfoundry-") || hop_by_hop.contains(name) {
            continue;
        }
        forwarded.append(key.clone(), value.clone());
    }
    forwarded
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BodyTooLarge;

pub(crate) fn request_body_limit(tenant: Option<&TenantContext>) -> usize {
    tenant
        .map(|tenant| tenant.quotas.max_request_body_bytes.max(1))
        .unwrap_or(10 * 1024 * 1024)
}

pub(crate) fn content_length_exceeds(headers: &HeaderMap, limit: usize) -> bool {
    headers
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > limit as u64)
}

pub(crate) fn take_chunk(remaining: &mut usize, chunk_len: usize) -> Result<(), BodyTooLarge> {
    if chunk_len > *remaining {
        return Err(BodyTooLarge);
    }
    *remaining -= chunk_len;
    Ok(())
}

/// Recreate tenant headers from the decoded JWT only.
pub(crate) fn apply_tenant_trust_headers(headers: &mut HeaderMap, tenant: &TenantContext) {
    insert_trusted_header(headers, "x-openfoundry-tenant-scope", &tenant.scope_id);
    insert_trusted_header(headers, "x-openfoundry-tenant-tier", &tenant.tier);
    insert_trusted_header(
        headers,
        "x-openfoundry-quota-query-limit",
        &tenant.quotas.max_query_limit.to_string(),
    );
    insert_trusted_header(
        headers,
        "x-openfoundry-quota-pipeline-workers",
        &tenant.quotas.max_pipeline_workers.to_string(),
    );
    insert_trusted_header(
        headers,
        "x-openfoundry-quota-requests-per-minute",
        &tenant.quotas.requests_per_minute.to_string(),
    );
}

/// Reverse-proxy handler: forwards requests to backend services based on URL prefix.
pub async fn proxy_handler(
    State((config, client)): State<(GatewayConfig, Client)>,
    mut req: Request,
) -> Response {
    let path = req.uri().path();
    let tenant = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(|token| jwt::decode_token(&JwtConfig::new(&config.jwt_secret), token).ok())
        .map(|claims| TenantContext::from_claims(&claims));

    let upstream_base = if path.starts_with("/api/v1/auth") {
        &config.auth_service_url
    } else if path.starts_with("/api/v1/datasets") {
        &config.dataset_service_url
    } else if path.starts_with("/api/v1/queries") {
        &config.query_service_url
    } else if path.starts_with("/api/v1/pipelines") {
        &config.pipeline_service_url
    } else if path.starts_with("/api/v1/ontology") {
        &config.ontology_service_url
    } else if path.starts_with("/api/v1/workflows") {
        &config.workflow_service_url
    } else if path.starts_with("/api/v1/notifications") {
        &config.notification_service_url
    } else if path.starts_with("/api/v1/ml") {
        &config.ml_service_url
    } else if path.starts_with("/api/v1/ai") {
        &config.ai_service_url
    } else if path.starts_with("/api/v1/fusion") {
        &config.fusion_service_url
    } else if path.starts_with("/api/v1/streaming") {
		&config.streaming_service_url
        } else if path.starts_with("/api/v1/reports") {
		&config.report_service_url
        } else if path.starts_with("/api/v1/geospatial") {
		&config.geospatial_service_url
        } else if path.starts_with("/api/v1/code-repos") {
		&config.code_repo_service_url
        } else if path.starts_with("/api/v1/marketplace") {
		&config.marketplace_service_url
        } else if path.starts_with("/api/v1/audit") {
		&config.audit_service_url
    } else if path.starts_with("/api/v1/nexus") {
		&config.nexus_service_url
    } else if path.starts_with("/api/v1/apps") || path.starts_with("/api/v1/widgets") {
        &config.app_builder_service_url
    } else {
        return (StatusCode::NOT_FOUND, "unknown service route").into_response();
    };

    let upstream_base = service_runtime::rewrite_upstream_base(upstream_base, config.tls_mode);
    let uri = format!("{upstream_base}{}", req.uri().path_and_query().map(|pq| pq.as_str()).unwrap_or("/"));

    let Ok(uri) = uri.parse::<Uri>() else {
        return (StatusCode::BAD_GATEWAY, "invalid upstream URI").into_response();
    };
    *req.uri_mut() = uri;

    let method = req.method().clone();
    let url = req.uri().to_string();
    let mut headers = sanitize_forward_headers(req.headers());
    if let Some(tenant) = tenant.as_ref() {
        apply_tenant_trust_headers(&mut headers, tenant);
    }
    let body_limit = request_body_limit(tenant.as_ref());
    if content_length_exceeds(&headers, body_limit) {
        return (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response();
    }

    let overflow = Arc::new(AtomicBool::new(false));
    let remaining = Arc::new(Mutex::new(body_limit));
    let overflow_flag = overflow.clone();
    let remaining_flag = remaining.clone();
    let stream = req.into_body().into_data_stream().map(move |chunk| {
        let bytes = chunk.map_err(std::io::Error::other)?;
        let mut remaining = remaining_flag
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if take_chunk(&mut remaining, bytes.len()).is_err() {
            overflow_flag.store(true, Ordering::SeqCst);
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "body too large",
            ));
        }
        Ok(bytes)
    });

    let mut upstream_req = client.request(method, &url);
    for (key, value) in headers.iter() {
        upstream_req = upstream_req.header(key, value);
    }
    upstream_req = upstream_req.body(reqwest::Body::wrap_stream(stream));

    match upstream_req.send().await {
        Ok(resp) => {
            let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
            let headers = sanitize_forward_headers(resp.headers());
            let stream = resp.bytes_stream();
            let mut response = Response::builder().status(status);
            for (key, value) in headers.iter() {
                response = response.header(key, value);
            }
            response
                .body(Body::from_stream(stream))
                .unwrap_or_else(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, "proxy error").into_response()
                })
        }
        Err(_) if overflow.load(Ordering::SeqCst) => {
            (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response()
        }
        Err(e) => {
            tracing::error!("upstream request failed: {e}");
            (StatusCode::BAD_GATEWAY, "upstream unavailable").into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use auth_middleware::tenant::TenantQuotaPolicy;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (key, value) in pairs {
            map.append(
                HeaderName::from_bytes(key.as_bytes()).expect("header name"),
                HeaderValue::from_str(value).expect("header value"),
            );
        }
        map
    }

    #[test]
    fn strips_client_supplied_openfoundry_headers() {
        let incoming = headers(&[
            ("authorization", "Bearer client-token"),
            ("x-openfoundry-tenant-scope", "attacker-scope"),
            ("x-openfoundry-tenant-tier", "enterprise"),
            ("x-openfoundry-quota-query-limit", "999999"),
            ("X-OpenFoundry-Quota-Requests-Per-Minute", "1"),
            ("content-type", "application/json"),
        ]);

        let sanitized = sanitize_forward_headers(&incoming);

        assert!(sanitized.get("x-openfoundry-tenant-scope").is_none());
        assert!(sanitized.get("x-openfoundry-tenant-tier").is_none());
        assert!(sanitized.get("x-openfoundry-quota-query-limit").is_none());
        assert!(sanitized.get("x-openfoundry-quota-requests-per-minute").is_none());
        assert_eq!(
            sanitized.get(AUTHORIZATION).and_then(|value| value.to_str().ok()),
            Some("Bearer client-token")
        );
        assert_eq!(
            sanitized
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn strips_hop_by_hop_headers_including_connection_list() {
        let incoming = headers(&[
            ("host", "evil.example"),
            ("connection", "keep-alive, x-custom-hop"),
            ("keep-alive", "timeout=5"),
            ("proxy-authorization", "Basic abc"),
            ("transfer-encoding", "chunked"),
            ("upgrade", "websocket"),
            ("te", "trailers"),
            ("x-custom-hop", "should-drop"),
            ("accept", "application/json"),
        ]);

        let sanitized = sanitize_forward_headers(&incoming);

        assert!(sanitized.get("host").is_none());
        assert!(sanitized.get("connection").is_none());
        assert!(sanitized.get("keep-alive").is_none());
        assert!(sanitized.get("proxy-authorization").is_none());
        assert!(sanitized.get("transfer-encoding").is_none());
        assert!(sanitized.get("upgrade").is_none());
        assert!(sanitized.get("te").is_none());
        assert!(sanitized.get("x-custom-hop").is_none());
        assert_eq!(
            sanitized.get("accept").and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn apply_tenant_headers_writes_only_trusted_values() {
        let mut forwarded = sanitize_forward_headers(&headers(&[
            ("x-openfoundry-tenant-scope", "attacker-scope"),
            ("accept", "application/json"),
        ]));
        let tenant = TenantContext {
            tenant_id: None,
            scope_id: "trusted-scope".into(),
            tier: "standard".into(),
            workspace: None,
            quotas: TenantQuotaPolicy::standard(),
        };

        apply_tenant_trust_headers(&mut forwarded, &tenant);

        assert_eq!(
            forwarded
                .get("x-openfoundry-tenant-scope")
                .and_then(|value| value.to_str().ok()),
            Some("trusted-scope")
        );
        assert_eq!(
            forwarded
                .get("x-openfoundry-tenant-tier")
                .and_then(|value| value.to_str().ok()),
            Some("standard")
        );
        assert_eq!(
            forwarded
                .get("x-openfoundry-quota-query-limit")
                .and_then(|value| value.to_str().ok()),
            Some("2000")
        );
        assert_eq!(
            forwarded
                .get("accept")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    fn request_body_limit_uses_tenant_clamp() {
        let tenant = TenantContext {
            tenant_id: None,
            scope_id: "scope".into(),
            tier: "standard".into(),
            workspace: None,
            quotas: TenantQuotaPolicy::standard(),
        };
        assert_eq!(request_body_limit(Some(&tenant)), 10 * 1024 * 1024);
        assert_eq!(request_body_limit(None), 10 * 1024 * 1024);

        let mut small = TenantQuotaPolicy::standard();
        small.max_request_body_bytes = 1024;
        let tenant = TenantContext {
            quotas: small,
            ..tenant
        };
        assert_eq!(request_body_limit(Some(&tenant)), 1024);
    }

    #[test]
    fn content_length_exceeds_declared_limit() {
        let over = headers(&[("content-length", "2048")]);
        let under = headers(&[("content-length", "512")]);
        let missing = headers(&[]);
        assert!(content_length_exceeds(&over, 1024));
        assert!(!content_length_exceeds(&under, 1024));
        assert!(!content_length_exceeds(&missing, 1024));
    }

    #[test]
    fn take_chunk_rejects_when_remaining_bytes_are_exhausted() {
        let mut remaining = 8;
        assert!(take_chunk(&mut remaining, 5).is_ok());
        assert_eq!(remaining, 3);
        assert_eq!(take_chunk(&mut remaining, 4), Err(BodyTooLarge));
        assert_eq!(remaining, 3);
    }
}
