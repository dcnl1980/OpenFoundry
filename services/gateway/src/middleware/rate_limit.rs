use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::{
	extract::{Request, State},
	http::{header::AUTHORIZATION, HeaderMap, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
};
use auth_middleware::{jwt, tenant::TenantContext, tenant::TenantQuotaPolicy, JwtConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateDecision {
	Allowed,
	Limited { retry_after_secs: u64 },
}

#[derive(Debug, Clone)]
struct Bucket {
	tokens: f64,
	last_refill: Instant,
	rate_per_minute: u32,
}

#[derive(Clone, Default)]
pub struct TokenBucketLimiter {
	buckets: Arc<Mutex<HashMap<String, Bucket>>>,
}

impl TokenBucketLimiter {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn check(&self, key: &str, requests_per_minute: u32) -> RateDecision {
		self.check_at(key, requests_per_minute, Instant::now())
	}

	pub(crate) fn check_at(
		&self,
		key: &str,
		requests_per_minute: u32,
		now: Instant,
	) -> RateDecision {
		let rate = requests_per_minute.max(1);
		let mut buckets = self
			.buckets
			.lock()
			.unwrap_or_else(|poisoned| poisoned.into_inner());
		let bucket = buckets.entry(key.to_string()).or_insert_with(|| Bucket {
			tokens: f64::from(rate),
			last_refill: now,
			rate_per_minute: rate,
		});
		if bucket.rate_per_minute != rate {
			bucket.rate_per_minute = rate;
		}

		let elapsed = now.saturating_duration_since(bucket.last_refill).as_secs_f64();
		let refill = elapsed * f64::from(bucket.rate_per_minute) / 60.0;
		bucket.tokens = (bucket.tokens + refill).min(f64::from(bucket.rate_per_minute));
		bucket.last_refill = now;

		if bucket.tokens >= 1.0 {
			bucket.tokens -= 1.0;
			RateDecision::Allowed
		} else {
			let needed = 1.0 - bucket.tokens;
			let retry_after_secs =
				((needed * 60.0) / f64::from(bucket.rate_per_minute)).ceil() as u64;
			RateDecision::Limited {
				retry_after_secs: retry_after_secs.max(1),
			}
		}
	}
}

#[derive(Clone)]
pub struct RateLimitState {
	pub jwt_secret: String,
	pub limiter: TokenBucketLimiter,
}

impl RateLimitState {
	pub fn new(jwt_secret: impl Into<String>) -> Self {
		Self {
			jwt_secret: jwt_secret.into(),
			limiter: TokenBucketLimiter::new(),
		}
	}
}

pub(crate) fn is_rate_limit_exempt(path: &str) -> bool {
	path == "/health"
}

pub(crate) fn rate_limit_key(tenant_scope: Option<&str>, client_ip: Option<&str>) -> String {
	if let Some(scope) = tenant_scope {
		format!("tenant:{scope}")
	} else if let Some(ip) = client_ip {
		format!("ip:{ip}")
	} else {
		"anonymous".to_string()
	}
}

pub(crate) fn client_ip(headers: &HeaderMap) -> Option<String> {
	headers
		.get("x-forwarded-for")
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.split(',').next())
		.map(str::trim)
		.filter(|value| !value.is_empty())
		.map(ToString::to_string)
		.or_else(|| {
			headers
				.get("x-real-ip")
				.and_then(|value| value.to_str().ok())
				.map(str::trim)
				.filter(|value| !value.is_empty())
				.map(ToString::to_string)
		})
}

fn tenant_from_headers(jwt_secret: &str, headers: &HeaderMap) -> Option<TenantContext> {
	headers
		.get(AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.and_then(|token| jwt::decode_token(&JwtConfig::new(jwt_secret), token).ok())
		.map(|claims| TenantContext::from_claims(&claims))
}

pub async fn rate_limit_layer(
	State(state): State<RateLimitState>,
	req: Request,
	next: Next,
) -> Response {
	if is_rate_limit_exempt(req.uri().path()) {
		return next.run(req).await;
	}

	let tenant = tenant_from_headers(&state.jwt_secret, req.headers());
	let key = rate_limit_key(
		tenant.as_ref().map(|tenant| tenant.scope_id.as_str()),
		client_ip(req.headers()).as_deref(),
	);
	let rpm = tenant
		.as_ref()
		.map(|tenant| tenant.quotas.requests_per_minute)
		.unwrap_or_else(|| TenantQuotaPolicy::standard().requests_per_minute);

	match state.limiter.check(&key, rpm) {
		RateDecision::Allowed => next.run(req).await,
		RateDecision::Limited { retry_after_secs } => (
			StatusCode::TOO_MANY_REQUESTS,
			[(
				axum::http::header::RETRY_AFTER,
				retry_after_secs.to_string(),
			)],
			"rate limit exceeded",
		)
			.into_response(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::http::{HeaderName, HeaderValue};
	use std::time::Duration;

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
	fn allows_requests_under_the_limit() {
		let limiter = TokenBucketLimiter::new();
		let now = Instant::now();
		assert_eq!(limiter.check_at("tenant:a", 2, now), RateDecision::Allowed);
		assert_eq!(limiter.check_at("tenant:a", 2, now), RateDecision::Allowed);
	}

	#[test]
	fn rejects_requests_after_the_bucket_is_empty() {
		let limiter = TokenBucketLimiter::new();
		let now = Instant::now();
		assert_eq!(limiter.check_at("tenant:a", 1, now), RateDecision::Allowed);
		assert_eq!(
			limiter.check_at("tenant:a", 1, now),
			RateDecision::Limited {
				retry_after_secs: 60
			}
		);
	}

	#[test]
	fn refills_tokens_as_time_passes() {
		let limiter = TokenBucketLimiter::new();
		let now = Instant::now();
		assert_eq!(limiter.check_at("tenant:a", 1, now), RateDecision::Allowed);
		assert!(matches!(
			limiter.check_at("tenant:a", 1, now),
			RateDecision::Limited { .. }
		));
		assert_eq!(
			limiter.check_at("tenant:a", 1, now + Duration::from_secs(60)),
			RateDecision::Allowed
		);
	}

	#[test]
	fn isolates_buckets_by_key() {
		let limiter = TokenBucketLimiter::new();
		let now = Instant::now();
		assert_eq!(limiter.check_at("tenant:a", 1, now), RateDecision::Allowed);
		assert_eq!(limiter.check_at("tenant:b", 1, now), RateDecision::Allowed);
		assert!(matches!(
			limiter.check_at("tenant:a", 1, now),
			RateDecision::Limited { .. }
		));
	}

	#[test]
	fn health_path_is_exempt() {
		assert!(is_rate_limit_exempt("/health"));
		assert!(!is_rate_limit_exempt("/api/v1/datasets"));
	}

	#[test]
	fn key_prefers_tenant_scope_then_ip() {
		assert_eq!(
			rate_limit_key(Some("org-1"), Some("1.2.3.4")),
			"tenant:org-1"
		);
		assert_eq!(rate_limit_key(None, Some("1.2.3.4")), "ip:1.2.3.4");
		assert_eq!(rate_limit_key(None, None), "anonymous");
	}

	#[test]
	fn client_ip_reads_forwarded_then_real_ip() {
		assert_eq!(
			client_ip(&headers(&[("x-forwarded-for", "1.2.3.4, 10.0.0.1")])).as_deref(),
			Some("1.2.3.4")
		);
		assert_eq!(
			client_ip(&headers(&[("x-real-ip", "8.8.8.8")])).as_deref(),
			Some("8.8.8.8")
		);
	}
}
