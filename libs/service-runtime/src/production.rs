//! Production boot checks. Development stays plaintext; production must not.

const DEFAULT_JWT_SECRET: &str = "change-me-in-production-use-a-256-bit-key";
const MIN_JWT_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEnvironment {
	Development,
	Production,
}

impl RuntimeEnvironment {
	pub fn from_value(value: Option<&str>) -> Self {
		match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
			Some("production") | Some("prod") => Self::Production,
			_ => Self::Development,
		}
	}

	pub fn from_env() -> Self {
		Self::from_value(std::env::var("ENVIRONMENT").ok().as_deref())
	}

	pub fn is_production(self) -> bool {
		matches!(self, Self::Production)
	}
}

pub fn insecure_jwt_secret_reason(secret: &str) -> Option<&'static str> {
	let trimmed = secret.trim();
	if trimmed.is_empty() {
		return Some("JWT_SECRET is empty");
	}
	if trimmed == DEFAULT_JWT_SECRET || trimmed.eq_ignore_ascii_case("secret") {
		return Some("JWT_SECRET is a published default and must be replaced");
	}
	if trimmed.len() < MIN_JWT_SECRET_LEN {
		return Some("JWT_SECRET must be at least 32 characters");
	}
	None
}

pub fn validate_jwt_secret(secret: &str, environment: RuntimeEnvironment) -> Result<(), String> {
	if !environment.is_production() {
		return Ok(());
	}
	match insecure_jwt_secret_reason(secret) {
		Some(reason) => Err(format!("{reason} before starting in production")),
		None => Ok(()),
	}
}

pub fn validate_tls_for_environment(
	mode: super::TlsMode,
	environment: RuntimeEnvironment,
) -> Result<(), String> {
	if !environment.is_production() {
		return Ok(());
	}
	if mode.requires_client_cert() {
		return Ok(());
	}
	Err(
		"ENVIRONMENT=production requires mTLS (TLS_CERT_PATH, TLS_KEY_PATH, and TLS_CA_PATH)"
			.into(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::TlsMode;

	#[test]
	fn production_aliases_are_recognized() {
		assert_eq!(
			RuntimeEnvironment::from_value(Some("production")),
			RuntimeEnvironment::Production
		);
		assert_eq!(
			RuntimeEnvironment::from_value(Some("PROD")),
			RuntimeEnvironment::Production
		);
		assert_eq!(
			RuntimeEnvironment::from_value(Some("development")),
			RuntimeEnvironment::Development
		);
		assert_eq!(
			RuntimeEnvironment::from_value(None),
			RuntimeEnvironment::Development
		);
	}

	#[test]
	fn default_and_short_jwt_secrets_are_rejected() {
		assert!(insecure_jwt_secret_reason(DEFAULT_JWT_SECRET).is_some());
		assert!(insecure_jwt_secret_reason("secret").is_some());
		assert!(insecure_jwt_secret_reason("   ").is_some());
		assert!(insecure_jwt_secret_reason("short-secret").is_some());
		assert!(insecure_jwt_secret_reason("abcdefghijklmnopqrstuvwxyz012345").is_none());
	}

	#[test]
	fn development_allows_default_jwt_secret() {
		assert!(validate_jwt_secret(DEFAULT_JWT_SECRET, RuntimeEnvironment::Development).is_ok());
		assert!(validate_jwt_secret(DEFAULT_JWT_SECRET, RuntimeEnvironment::Production).is_err());
	}

	#[test]
	fn production_requires_mutual_tls() {
		assert!(validate_tls_for_environment(TlsMode::Disabled, RuntimeEnvironment::Development).is_ok());
		assert!(validate_tls_for_environment(TlsMode::Disabled, RuntimeEnvironment::Production).is_err());
		assert!(validate_tls_for_environment(TlsMode::ServerOnly, RuntimeEnvironment::Production).is_err());
		assert!(validate_tls_for_environment(TlsMode::Mutual, RuntimeEnvironment::Production).is_ok());
	}
}
