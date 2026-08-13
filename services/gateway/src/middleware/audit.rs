use axum::{
	extract::{Request, State},
	http::Uri,
	middleware::Next,
	response::Response,
};
use event_bus::{
	subscriber,
	topics::{subjects, streams},
	Publisher,
};
use serde::Serialize;

const AUDIT_QUEUE_CAPACITY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnqueueOutcome {
	Queued,
	Dropped,
	Disabled,
}

#[derive(Clone, Default)]
pub struct AuditHandle {
	tx: Option<tokio::sync::mpsc::Sender<GatewayAuditPayload>>,
}

impl AuditHandle {
	pub fn disabled() -> Self {
		Self { tx: None }
	}

	pub(crate) fn with_sender(tx: tokio::sync::mpsc::Sender<GatewayAuditPayload>) -> Self {
		Self { tx: Some(tx) }
	}

	pub(crate) fn try_enqueue(&self, payload: GatewayAuditPayload) -> EnqueueOutcome {
		match &self.tx {
			None => EnqueueOutcome::Disabled,
			Some(tx) => match tx.try_send(payload) {
				Ok(()) => EnqueueOutcome::Queued,
				Err(_) => EnqueueOutcome::Dropped,
			},
		}
	}
}

/// Path recorded in gateway audit events. Query strings are omitted so
/// tokens, search terms, and identifiers are not written to the bus.
pub(crate) fn audit_request_path(uri: &Uri) -> String {
	let path = uri.path();
	if path.is_empty() {
		"/".to_string()
	} else {
		path.to_string()
	}
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GatewayAuditPayload {
	source_service: String,
	channel: String,
	actor: String,
	action: String,
	resource_type: String,
	resource_id: String,
	status: &'static str,
	severity: &'static str,
	classification: &'static str,
	subject_id: Option<String>,
	ip_address: Option<String>,
	location: Option<String>,
	metadata: GatewayAuditMetadata,
	labels: Vec<String>,
	retention_days: i32,
}

#[derive(Debug, Clone, Serialize)]
struct GatewayAuditMetadata {
	request_id: String,
	method: String,
	path: String,
	status: u16,
	user_agent: Option<String>,
}

pub async fn connect_audit_handle(nats_url: Option<&str>) -> AuditHandle {
	let Some(url) = nats_url else {
		return AuditHandle::disabled();
	};

	match event_bus::connect(url).await {
		Ok(js) => {
			if let Err(cause) =
				subscriber::ensure_stream(&js, streams::AUDIT, &[subjects::AUDIT]).await
			{
				tracing::warn!(?cause, "failed to ensure audit stream");
				return AuditHandle::disabled();
			}

			let publisher = Publisher::new(js, "gateway");
			let (tx, rx) = tokio::sync::mpsc::channel(AUDIT_QUEUE_CAPACITY);
			tokio::spawn(run_audit_worker(publisher, rx));
			AuditHandle::with_sender(tx)
		}
		Err(cause) => {
			tracing::warn!(?cause, "failed to connect to NATS for audit publishing");
			AuditHandle::disabled()
		}
	}
}

async fn run_audit_worker(
	publisher: Publisher,
	mut rx: tokio::sync::mpsc::Receiver<GatewayAuditPayload>,
) {
	let subject = format!("{}.gateway", subjects::AUDIT);
	while let Some(payload) = rx.recv().await {
		if publish_audit(&publisher, &subject, &payload).await {
			continue;
		}
		tracing::warn!("retrying gateway audit publish once");
		if !publish_audit(&publisher, &subject, &payload).await {
			tracing::warn!("failed to publish gateway audit event after retry");
		}
	}
}

async fn publish_audit(publisher: &Publisher, subject: &str, payload: &GatewayAuditPayload) -> bool {
	match publisher
		.publish(subject, "audit.gateway.request.forwarded", payload)
		.await
	{
		Ok(()) => true,
		Err(cause) => {
			tracing::warn!(?cause, "failed to publish gateway audit event");
			false
		}
	}
}

pub async fn audit_layer(State(audit): State<AuditHandle>, req: Request, next: Next) -> Response {
	let request_id = req
		.headers()
		.get("x-request-id")
		.and_then(|value| value.to_str().ok())
		.unwrap_or("unknown")
		.to_string();
	let method = req.method().to_string();
	let path = audit_request_path(req.uri());
	let user_agent = req
		.headers()
		.get(axum::http::header::USER_AGENT)
		.and_then(|value| value.to_str().ok())
		.map(ToString::to_string);

	let response = next.run(req).await;
	let status = response.status().as_u16();

	let (event_status, severity) = if status >= 500 {
		("failure", "critical")
	} else if status >= 400 {
		("failure", "high")
	} else {
		("success", "low")
	};
	let payload = GatewayAuditPayload {
		source_service: "gateway".to_string(),
		channel: "nats".to_string(),
		actor: "system:gateway".to_string(),
		action: "request.forwarded".to_string(),
		resource_type: "http_request".to_string(),
		resource_id: path.clone(),
		status: event_status,
		severity,
		classification: "confidential",
		subject_id: None,
		ip_address: None,
		location: None,
		metadata: GatewayAuditMetadata {
			request_id,
			method,
			path,
			status,
			user_agent,
		},
		labels: vec!["auto-captured".to_string(), "gateway".to_string()],
		retention_days: 365,
	};

	match audit.try_enqueue(payload) {
		EnqueueOutcome::Dropped => {
			tracing::warn!("dropping gateway audit event: queue full");
		}
		EnqueueOutcome::Queued | EnqueueOutcome::Disabled => {}
	}

	response
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_payload(path: &str) -> GatewayAuditPayload {
		GatewayAuditPayload {
			source_service: "gateway".to_string(),
			channel: "nats".to_string(),
			actor: "system:gateway".to_string(),
			action: "request.forwarded".to_string(),
			resource_type: "http_request".to_string(),
			resource_id: path.to_string(),
			status: "success",
			severity: "low",
			classification: "confidential",
			subject_id: None,
			ip_address: None,
			location: None,
			metadata: GatewayAuditMetadata {
				request_id: "req-1".to_string(),
				method: "GET".to_string(),
				path: path.to_string(),
				status: 200,
				user_agent: None,
			},
			labels: vec!["gateway".to_string()],
			retention_days: 365,
		}
	}

	#[test]
	fn audit_path_omits_query_string() {
		let uri: Uri = "/api/v1/datasets?api_key=secret&q=p53"
			.parse()
			.expect("uri");
		assert_eq!(audit_request_path(&uri), "/api/v1/datasets");
	}

	#[test]
	fn audit_path_keeps_path_without_query() {
		let uri: Uri = "/health".parse().expect("uri");
		assert_eq!(audit_request_path(&uri), "/health");
	}

	#[test]
	fn disabled_handle_does_not_enqueue() {
		let handle = AuditHandle::disabled();
		assert_eq!(
			handle.try_enqueue(sample_payload("/health")),
			EnqueueOutcome::Disabled
		);
	}

	#[tokio::test]
	async fn drops_audit_events_when_queue_is_full() {
		let (tx, _rx) = tokio::sync::mpsc::channel(1);
		let handle = AuditHandle::with_sender(tx);
		assert_eq!(
			handle.try_enqueue(sample_payload("/one")),
			EnqueueOutcome::Queued
		);
		assert_eq!(
			handle.try_enqueue(sample_payload("/two")),
			EnqueueOutcome::Dropped
		);
	}
}
