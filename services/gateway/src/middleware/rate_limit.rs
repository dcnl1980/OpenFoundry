use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::{
	extract::{ConnectInfo, Request, State},
	http::{header::AUTHORIZATION, HeaderMap, StatusCode},
	middleware::Next,
	response::{IntoResponse, Response},
};
use auth_middleware::{jwt, tenant::TenantContext, JwtConfig};

use crate::config::GatewayConfig;

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

#[derive(Clone)]
pub struct TokenBucketLimiter {
	buckets: Arc<Mutex<HashMap<String, Bucket>>>,
	max_buckets: usize,
}

impl Default for TokenBucketLimiter {
	fn default() -> Self {
		Self::with_capacity(10_000)
	}
}

impl TokenBucketLimiter {
	pub fn new() -> Self {
		Self::default()
	}

	pub(crate) fn with_capacity(max_buckets: usize) -> Self {
		Self {
			buckets: Arc::new(Mutex::new(HashMap::new())),
			max_buckets: max_buckets.max(1),
		}
	}

	pub(crate) fn bucket_count(&self) -> usize {
		self.buckets
			.lock()
			.map(|buckets| buckets.len())
			.unwrap_or(0)
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
		if !buckets.contains_key(key) {
			evict_if_needed(&mut buckets, now, self.max_buckets);
		}
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

fn evict_if_needed(buckets: &mut HashMap<String, Bucket>, now: Instant, max_buckets: usize) {
	if buckets.len() < max_buckets {
		return;
	}
	buckets.retain(|_, bucket| {
		now.saturating_duration_since(bucket.last_refill) < Duration::from_secs(120)
	});
	while buckets.len() >= max_buckets {
		let oldest = buckets
			.iter()
			.min_by_key(|(_, bucket)| bucket.last_refill)
			.map(|(key, _)| key.clone());
		match oldest {
			Some(key) => {
				buckets.remove(&key);
			}
			None => break,
		}
	}
}

#[derive(Clone)]
pub struct RedisLimiter {
	connection: redis::aio::ConnectionManager,
}

impl RedisLimiter {
	pub async fn connect(url: &str) -> Result<Self, redis::RedisError> {
		let client = redis::Client::open(url)?;
		let connection = redis::aio::ConnectionManager::new(client).await?;
		Ok(Self { connection })
	}

	pub async fn check(
		&self,
		key: &str,
		requests_per_minute: u32,
	) -> Result<RateDecision, redis::RedisError> {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs();
		let window = now / 60;
		let retry_after = 60 - (now % 60);
		let redis_key = format!("of:rl:{key}:{window}");
		let mut connection = self.connection.clone();
		let count: u64 = redis::cmd("INCR")
			.arg(&redis_key)
			.query_async(&mut connection)
			.await?;
		if count == 1 {
			let _: () = redis::cmd("EXPIRE")
				.arg(&redis_key)
				.arg(120)
				.query_async(&mut connection)
				.await?;
		}
		Ok(fixed_window_decision(
			count,
			requests_per_minute,
			retry_after,
		))
	}
}

#[derive(Clone)]
pub struct RateLimitState {
	pub jwt_secret: String,
	pub trust_forwarded_headers: bool,
	pub anonymous_requests_per_minute: u32,
	pub limiter: TokenBucketLimiter,
	pub redis: Option<RedisLimiter>,
}

impl RateLimitState {
	pub async fn from_config(cfg: &GatewayConfig) -> Self {
		let redis = match cfg.redis_url.as_deref() {
			Some(url) => match RedisLimiter::connect(url).await {
				Ok(limiter) => {
					tracing::info!("gateway rate limiter using Redis");
					Some(limiter)
				}
				Err(cause) => {
					tracing::warn!(
						?cause,
						"failed to connect Redis rate limiter; using in-memory fallback"
					);
					None
				}
			},
			None => None,
		};
		Self {
			jwt_secret: cfg.jwt_secret.clone(),
			trust_forwarded_headers: cfg.trust_forwarded_headers,
			anonymous_requests_per_minute: cfg.anonymous_requests_per_minute.max(1),
			limiter: TokenBucketLimiter::new(),
			redis,
		}
	}

	async fn decide(&self, key: &str, requests_per_minute: u32) -> RateDecision {
		if let Some(redis) = &self.redis {
			match redis.check(key, requests_per_minute).await {
				Ok(decision) => return decision,
				Err(cause) => tracing::warn!(
					?cause,
					"Redis rate limit failed; falling back to in-memory limiter"
				),
			}
		}
		self.limiter.check(key, requests_per_minute)
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

pub(crate) fn client_ip(headers: &HeaderMap, trust_forwarded: bool) -> Option<String> {
	if !trust_forwarded {
		return None;
	}
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

pub(crate) fn fixed_window_decision(
	count: u64,
	requests_per_minute: u32,
	retry_after_secs: u64,
) -> RateDecision {
	if count <= u64::from(requests_per_minute.max(1)) {
		RateDecision::Allowed
	} else {
		RateDecision::Limited {
			retry_after_secs: retry_after_secs.max(1),
		}
	}
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
	let forwarded = client_ip(req.headers(), state.trust_forwarded_headers);
	let peer = req
		.extensions()
		.get::<ConnectInfo<SocketAddr>>()
		.map(|info| info.0.ip().to_string());
	let key = rate_limit_key(
		tenant.as_ref().map(|tenant| tenant.scope_id.as_str()),
		forwarded.as_deref().or(peer.as_deref()),
	);
	let rpm = tenant
		.as_ref()
		.map(|tenant| tenant.quotas.requests_per_minute)
		.unwrap_or(state.anonymous_requests_per_minute);

	match state.decide(&key, rpm).await {
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
			client_ip(&headers(&[("x-forwarded-for", "1.2.3.4, 10.0.0.1")]), true).as_deref(),
			Some("1.2.3.4")
		);
		assert_eq!(
			client_ip(&headers(&[("x-real-ip", "8.8.8.8")]), true).as_deref(),
			Some("8.8.8.8")
		);
	}

	#[test]
	fn client_ip_ignores_forwarded_headers_unless_trusted() {
		assert_eq!(
			client_ip(&headers(&[("x-forwarded-for", "1.2.3.4")]), false),
			None
		);
		assert_eq!(client_ip(&headers(&[("x-real-ip", "8.8.8.8")]), false), None);
	}

	#[test]
	fn evicts_stale_buckets_when_over_capacity() {
		let limiter = TokenBucketLimiter::with_capacity(2);
		let now = Instant::now();
		assert_eq!(limiter.check_at("a", 1, now), RateDecision::Allowed);
		assert_eq!(limiter.check_at("b", 1, now), RateDecision::Allowed);
		assert_eq!(
			limiter.check_at("c", 1, now + Duration::from_secs(180)),
			RateDecision::Allowed
		);
		assert_eq!(limiter.bucket_count(), 1);
	}

	#[test]
	fn fixed_window_rejects_counts_over_the_quota() {
		assert_eq!(
			fixed_window_decision(1, 2, 30),
			RateDecision::Allowed
		);
		assert_eq!(
			fixed_window_decision(2, 2, 30),
			RateDecision::Allowed
		);
		assert_eq!(
			fixed_window_decision(3, 2, 30),
			RateDecision::Limited {
				retry_after_secs: 30
			}
		);
	}
}
