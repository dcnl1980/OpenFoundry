use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

pub mod production;

const DEFAULT_BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsMode {
	#[default]
	Disabled,
	ServerOnly,
	Mutual,
}

impl TlsMode {
	pub fn uses_tls(self) -> bool {
		!matches!(self, Self::Disabled)
	}

	pub fn requires_client_cert(self) -> bool {
		matches!(self, Self::Mutual)
	}
}

#[derive(Debug, Clone, Default)]
pub struct TlsSettings {
	pub cert_path: Option<PathBuf>,
	pub key_path: Option<PathBuf>,
	pub ca_path: Option<PathBuf>,
}

impl TlsSettings {
	pub fn from_env() -> Self {
		Self::from_vars(
			std::env::var("TLS_CERT_PATH").ok(),
			std::env::var("TLS_KEY_PATH").ok(),
			std::env::var("TLS_CA_PATH").ok(),
		)
	}

	pub fn from_vars(
		cert_path: Option<String>,
		key_path: Option<String>,
		ca_path: Option<String>,
	) -> Self {
		Self {
			cert_path: nonempty_path(cert_path),
			key_path: nonempty_path(key_path),
			ca_path: nonempty_path(ca_path),
		}
	}

	pub fn mode(&self) -> TlsMode {
		match (&self.cert_path, &self.key_path, &self.ca_path) {
			(Some(_), Some(_), Some(_)) => TlsMode::Mutual,
			(Some(_), Some(_), None) => TlsMode::ServerOnly,
			_ => TlsMode::Disabled,
		}
	}
}

fn nonempty_path(value: Option<String>) -> Option<PathBuf> {
	value
		.filter(|value| !value.trim().is_empty())
		.map(PathBuf::from)
}

pub fn rewrite_upstream_base(url: &str, mode: TlsMode) -> Cow<'_, str> {
	if mode.uses_tls() {
		if let Some(rest) = url.strip_prefix("http://") {
			return Cow::Owned(format!("https://{rest}"));
		}
	}
	Cow::Borrowed(url)
}

pub fn default_body_limit_bytes() -> usize {
	DEFAULT_BODY_LIMIT_BYTES
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
	#[error("missing TLS file {0}")]
	MissingFile(String),
	#[error("failed to read {path}: {source}")]
	Read {
		path: String,
		#[source]
		source: std::io::Error,
	},
	#[error("invalid PEM in {0}")]
	InvalidPem(String),
	#[error("TLS configuration error: {0}")]
	Config(String),
}

pub fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsError> {
	let bytes = std::fs::read(path).map_err(|source| TlsError::Read {
		path: path.display().to_string(),
		source,
	})?;
	let mut reader = std::io::Cursor::new(bytes);
	let certs = rustls_pemfile::certs(&mut reader)
		.collect::<Result<Vec<_>, _>>()
		.map_err(|_| TlsError::InvalidPem(path.display().to_string()))?;
	if certs.is_empty() {
		return Err(TlsError::InvalidPem(path.display().to_string()));
	}
	Ok(certs)
}

pub fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsError> {
	let bytes = std::fs::read(path).map_err(|source| TlsError::Read {
		path: path.display().to_string(),
		source,
	})?;
	let mut reader = std::io::Cursor::new(bytes);
	if let Some(key) = rustls_pemfile::private_key(&mut reader)
		.map_err(|_| TlsError::InvalidPem(path.display().to_string()))?
	{
		return Ok(key);
	}
	Err(TlsError::InvalidPem(path.display().to_string()))
}

fn install_crypto_provider() {
	let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn server_config(settings: &TlsSettings) -> Result<ServerConfig, TlsError> {
	install_crypto_provider();
	let cert_path = settings
		.cert_path
		.as_deref()
		.ok_or_else(|| TlsError::MissingFile("TLS_CERT_PATH".into()))?;
	let key_path = settings
		.key_path
		.as_deref()
		.ok_or_else(|| TlsError::MissingFile("TLS_KEY_PATH".into()))?;
	let certs = load_certificates(cert_path)?;
	let key = load_private_key(key_path)?;

	let builder = match settings.ca_path.as_deref() {
		Some(ca_path) => {
			let mut roots = RootCertStore::empty();
			for cert in load_certificates(ca_path)? {
				roots
					.add(cert)
					.map_err(|error| TlsError::Config(error.to_string()))?;
			}
			let verifier = WebPkiClientVerifier::builder(Arc::new(roots))
				.build()
				.map_err(|error| TlsError::Config(error.to_string()))?;
			ServerConfig::builder().with_client_cert_verifier(verifier)
		}
		None => ServerConfig::builder().with_no_client_auth(),
	};

	builder
		.with_single_cert(certs, key)
		.map_err(|error| TlsError::Config(error.to_string()))
}

pub fn configure_http_client(
	mut builder: reqwest::ClientBuilder,
	settings: &TlsSettings,
) -> Result<reqwest::Client, TlsError> {
	if let Some(ca_path) = settings.ca_path.as_deref() {
		let pem = std::fs::read(ca_path).map_err(|source| TlsError::Read {
			path: ca_path.display().to_string(),
			source,
		})?;
		let ca = reqwest::Certificate::from_pem(&pem)
			.map_err(|error| TlsError::Config(error.to_string()))?;
		builder = builder.add_root_certificate(ca);
	}

	if let (Some(cert_path), Some(key_path)) =
		(settings.cert_path.as_deref(), settings.key_path.as_deref())
	{
		let mut identity = std::fs::read(cert_path).map_err(|source| TlsError::Read {
			path: cert_path.display().to_string(),
			source,
		})?;
		identity.extend_from_slice(&std::fs::read(key_path).map_err(|source| TlsError::Read {
			path: key_path.display().to_string(),
			source,
		})?);
		let identity = reqwest::Identity::from_pem(&identity)
			.map_err(|error| TlsError::Config(error.to_string()))?;
		builder = builder.identity(identity);
	}

	builder
		.build()
		.map_err(|error| TlsError::Config(error.to_string()))
}

fn harden(app: Router) -> Router {
	app.layer(DefaultBodyLimit::max(DEFAULT_BODY_LIMIT_BYTES))
}

pub async fn serve(app: Router, addr: &str, settings: TlsSettings) -> Result<(), std::io::Error> {
	let app = harden(app);
	let addr: SocketAddr = addr
		.parse()
		.map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
	if let Err(reason) =
		production::validate_tls_for_environment(settings.mode(), production::RuntimeEnvironment::from_env())
	{
		return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, reason));
	}

	match settings.mode() {
		TlsMode::Disabled => {
			tracing::warn!(
				%addr,
				"TLS disabled; serving plaintext HTTP (development only)"
			);
			let listener = tokio::net::TcpListener::bind(addr).await?;
			axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await
		}
		mode => {
			let config = server_config(&settings)
				.map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))?;
			tracing::info!(%addr, ?mode, "serving with TLS");
			axum_server::bind_rustls(addr, axum_server::tls_rustls::RustlsConfig::from_config(Arc::new(config)))
				.serve(app.into_make_service_with_connect_info::<SocketAddr>())
				.await
		}
	}
}

// Keep PKCS8 type referenced so rustls-pemfile key loading stays compatible.
#[allow(dead_code)]
fn _pkcs8_marker(_: PrivatePkcs8KeyDer<'_>) {}

#[cfg(test)]
mod tests {
	use super::*;
	use rcgen::{CertificateParams, KeyPair};

	#[test]
	fn mode_is_mutual_only_when_cert_key_and_ca_are_set() {
		assert_eq!(TlsSettings::from_vars(None, None, None).mode(), TlsMode::Disabled);
		assert_eq!(
			TlsSettings::from_vars(Some("cert.pem".into()), Some("key.pem".into()), None).mode(),
			TlsMode::ServerOnly
		);
		assert_eq!(
			TlsSettings::from_vars(
				Some("cert.pem".into()),
				Some("key.pem".into()),
				Some("ca.pem".into())
			)
			.mode(),
			TlsMode::Mutual
		);
		assert_eq!(
			TlsSettings::from_vars(Some("cert.pem".into()), None, Some("ca.pem".into())).mode(),
			TlsMode::Disabled
		);
	}

	#[test]
	fn rewrite_upstream_base_upgrades_http_when_tls_is_on() {
		assert_eq!(
			rewrite_upstream_base("http://ontology:50057", TlsMode::Mutual),
			"https://ontology:50057"
		);
		assert_eq!(
			rewrite_upstream_base("https://ontology:50057", TlsMode::Mutual),
			"https://ontology:50057"
		);
		assert_eq!(
			rewrite_upstream_base("http://ontology:50057", TlsMode::Disabled),
			"http://ontology:50057"
		);
	}

	#[test]
	fn empty_env_values_are_treated_as_unset() {
		let settings = TlsSettings::from_vars(Some("  ".into()), Some("key.pem".into()), None);
		assert!(settings.cert_path.is_none());
	}

	#[test]
	fn loads_generated_pem_material_and_builds_mtls_server_config() {
		let dir = tempfile::tempdir().expect("tempdir");
		let key_pair = KeyPair::generate().expect("key");
		let params = CertificateParams::new(vec!["localhost".into()]).expect("params");
		let cert = params.self_signed(&key_pair).expect("cert");
		let cert_path = dir.path().join("cert.pem");
		let key_path = dir.path().join("key.pem");
		std::fs::write(&cert_path, cert.pem()).expect("write cert");
		std::fs::write(&key_path, key_pair.serialize_pem()).expect("write key");

		let settings = TlsSettings {
			cert_path: Some(cert_path.clone()),
			key_path: Some(key_path),
			ca_path: Some(cert_path),
		};
		assert_eq!(settings.mode(), TlsMode::Mutual);
		server_config(&settings).expect("server config");
	}
}
