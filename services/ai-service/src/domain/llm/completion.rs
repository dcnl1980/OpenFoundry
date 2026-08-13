use serde_json::{json, Value};

use crate::models::provider::LlmProvider;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionRequest {
	pub url: String,
	pub headers: Vec<(String, String)>,
	pub body: Value,
}

pub fn resolve_credential(
	reference: Option<&str>,
	getenv: impl Fn(&str) -> Option<String>,
) -> Option<String> {
	let reference = reference.map(str::trim).filter(|value| !value.is_empty())?;
	getenv(reference).filter(|value| !value.trim().is_empty()).or_else(|| {
		if reference.contains('=') || reference.contains('/') {
			None
		} else if reference.chars().all(|ch| ch.is_ascii_uppercase() || ch == '_') {
			None
		} else {
			Some(reference.to_string())
		}
	})
}

pub fn join_endpoint(base: &str, suffix: &str) -> String {
	format!(
		"{}/{}",
		base.trim_end_matches('/'),
		suffix.trim_start_matches('/')
	)
}

pub fn build_completion_request(
	provider: &LlmProvider,
	prompt: &str,
	credential: Option<&str>,
) -> Result<CompletionRequest, String> {
	if provider.endpoint_url.trim().is_empty() {
		return Err("provider endpoint_url is empty".into());
	}

	let (url, body, extra_headers) = match (provider.provider_type.as_str(), provider.api_mode.as_str())
	{
		("anthropic", _) | (_, "messages") => (
			join_endpoint(&provider.endpoint_url, "messages"),
			json!({
				"model": provider.model_name,
				"max_tokens": provider.max_output_tokens.max(1),
				"messages": [{ "role": "user", "content": prompt }],
			}),
			vec![("anthropic-version".to_string(), "2023-06-01".to_string())],
		),
		("ollama", _) | (_, "chat") => (
			join_endpoint(&provider.endpoint_url, "chat"),
			json!({
				"model": provider.model_name,
				"stream": false,
				"messages": [
					{ "role": "system", "content": "You are the OpenFoundry platform copilot." },
					{ "role": "user", "content": prompt }
				],
			}),
			Vec::new(),
		),
		_ => (
			join_endpoint(&provider.endpoint_url, "chat/completions"),
			json!({
				"model": provider.model_name,
				"max_tokens": provider.max_output_tokens.max(1),
				"messages": [
					{ "role": "system", "content": "You are the OpenFoundry platform copilot." },
					{ "role": "user", "content": prompt }
				],
			}),
			Vec::new(),
		),
	};

	let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
	headers.extend(extra_headers);
	if let Some(credential) = credential.map(str::trim).filter(|value| !value.is_empty()) {
		if provider.provider_type == "anthropic" || provider.api_mode == "messages" {
			headers.push(("x-api-key".to_string(), credential.to_string()));
		} else {
			headers.push(("authorization".to_string(), format!("Bearer {credential}")));
		}
	}

	Ok(CompletionRequest { url, headers, body })
}

pub fn parse_completion_response(provider_type: &str, api_mode: &str, body: &Value) -> Result<String, String> {
	let text = if provider_type == "anthropic" || api_mode == "messages" {
		body.get("content")
			.and_then(Value::as_array)
			.and_then(|blocks| {
				blocks.iter().find_map(|block| {
					block
						.get("text")
						.and_then(Value::as_str)
						.map(str::trim)
						.filter(|value| !value.is_empty())
						.map(ToOwned::to_owned)
				})
			})
	} else if provider_type == "ollama" || api_mode == "chat" {
		body.pointer("/message/content")
			.and_then(Value::as_str)
			.map(str::trim)
			.filter(|value| !value.is_empty())
			.map(ToOwned::to_owned)
	} else {
		body.pointer("/choices/0/message/content")
			.and_then(Value::as_str)
			.map(str::trim)
			.filter(|value| !value.is_empty())
			.map(ToOwned::to_owned)
	};

	text.ok_or_else(|| "provider response did not include completion text".into())
}

pub async fn complete(
	client: &reqwest::Client,
	provider: &LlmProvider,
	prompt: &str,
) -> Result<String, String> {
	let credential = resolve_credential(provider.credential_reference.as_deref(), |key| {
		std::env::var(key).ok()
	});
	let request = build_completion_request(provider, prompt, credential.as_deref())?;
	let mut builder = client.post(&request.url).json(&request.body);
	for (key, value) in &request.headers {
		builder = builder.header(key, value);
	}

	let response = builder
		.send()
		.await
		.map_err(|cause| format!("provider request failed: {cause}"))?;
	let status = response.status();
	let payload = response
		.json::<Value>()
		.await
		.map_err(|cause| format!("provider response was not JSON: {cause}"))?;
	if !status.is_success() {
		let detail = payload
			.get("error")
			.and_then(|error| error.get("message").or(Some(error)))
			.and_then(Value::as_str)
			.unwrap_or(status.as_str());
		return Err(format!("provider returned HTTP {status}: {detail}"));
	}

	parse_completion_response(&provider.provider_type, &provider.api_mode, &payload)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::provider::{LlmProvider, ProviderHealthState, ProviderRoutingRules};
	use chrono::Utc;
	use uuid::Uuid;

	fn provider(provider_type: &str, api_mode: &str, endpoint_url: &str) -> LlmProvider {
		LlmProvider {
			id: Uuid::now_v7(),
			name: "test".into(),
			provider_type: provider_type.into(),
			model_name: "gpt-test".into(),
			endpoint_url: endpoint_url.into(),
			api_mode: api_mode.into(),
			credential_reference: Some("OPENAI_API_KEY".into()),
			credential_configured: true,
			enabled: true,
			load_balance_weight: 100,
			max_output_tokens: 256,
			cost_tier: "standard".into(),
			tags: Vec::new(),
			route_rules: ProviderRoutingRules::default(),
			health_state: ProviderHealthState::default(),
			created_at: Utc::now(),
			updated_at: Utc::now(),
		}
	}

	#[test]
	fn resolve_credential_reads_env_names_and_ignores_missing_keys() {
		let value = resolve_credential(Some("OPENAI_API_KEY"), |key| {
			(key == "OPENAI_API_KEY").then(|| "sk-test".into())
		});
		assert_eq!(value.as_deref(), Some("sk-test"));
		assert_eq!(
			resolve_credential(Some("OPENAI_API_KEY"), |_| None),
			None
		);
	}

	#[test]
	fn build_openai_chat_completion_request() {
		let request = build_completion_request(
			&provider("openai", "chat_completions", "https://api.openai.com/v1"),
			"What encodes p53?",
			Some("sk-test"),
		)
		.expect("request should build");

		assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
		assert!(request
			.headers
			.iter()
			.any(|(key, value)| key == "authorization" && value == "Bearer sk-test"));
		assert_eq!(request.body["model"], "gpt-test");
		assert_eq!(request.body["messages"][1]["content"], "What encodes p53?");
	}

	#[test]
	fn parse_openai_and_anthropic_envelopes() {
		let openai = json!({
			"choices": [{ "message": { "content": "TP53 encodes p53." } }]
		});
		let anthropic = json!({
			"content": [{ "type": "text", "text": "TP53 encodes p53." }]
		});

		assert_eq!(
			parse_completion_response("openai", "chat_completions", &openai).unwrap(),
			"TP53 encodes p53."
		);
		assert_eq!(
			parse_completion_response("anthropic", "messages", &anthropic).unwrap(),
			"TP53 encodes p53."
		);
	}
}
