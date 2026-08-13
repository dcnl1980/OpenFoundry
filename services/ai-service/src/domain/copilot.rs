use uuid::Uuid;

use crate::models::knowledge_base::KnowledgeSearchResult;

#[derive(Debug, Clone)]
pub struct CopilotDraft {
	pub answer: String,
	pub suggested_sql: Option<String>,
	pub pipeline_suggestions: Vec<String>,
	pub ontology_hints: Vec<String>,
}

pub fn assist(
	question: &str,
	dataset_ids: &[Uuid],
	ontology_type_ids: &[Uuid],
	knowledge_hits: &[KnowledgeSearchResult],
	include_sql: bool,
	include_pipeline_plan: bool,
) -> CopilotDraft {
	let lowered = question.to_lowercase();
	let first_dataset = dataset_ids.first().copied();
	let first_ontology_type = ontology_type_ids.first().copied();

	let suggested_sql = if include_sql {
		if let Some(dataset_id) = first_dataset {
			Some(format!(
				"SELECT *\nFROM dataset_{}\nWHERE event_date >= CURRENT_DATE - INTERVAL '30 days'\nORDER BY event_date DESC\nLIMIT 100;",
				dataset_id.simple()
			))
		} else if lowered.contains("sql") || lowered.contains("query") {
			Some(
				"SELECT *\nFROM your_dataset\nWHERE created_at >= CURRENT_DATE - INTERVAL '7 days';"
					.to_string(),
			)
		} else {
			None
		}
	} else {
		None
	};

	let pipeline_suggestions = if include_pipeline_plan {
		vec![
			"Profile the incoming source and verify schema drift before inference.".to_string(),
			"Materialize embeddings and retrieval chunks as a scheduled upstream step.".to_string(),
			"Add a guardrail validation node before publishing generated outputs.".to_string(),
		]
	} else {
		Vec::new()
	};

	let mut ontology_hints = Vec::new();
	if let Some(object_type_id) = first_ontology_type {
		ontology_hints.push(format!(
			"Map the response to ontology type {} for downstream actioning.",
			object_type_id.simple()
		));
	}
	if lowered.contains("ontology") || lowered.contains("object") {
		ontology_hints.push(
			"Prefer stable object identifiers and link types when grounding answers.".to_string(),
		);
	}

	let knowledge_summary = if knowledge_hits.is_empty() {
		"No indexed knowledge passages were required for this answer.".to_string()
	} else {
		format!(
			"Retrieved {} supporting passage(s), starting with '{}'.",
			knowledge_hits.len(),
			knowledge_hits[0].document_title
		)
	};

	CopilotDraft {
		answer: format!(
			"Copilot reviewed the request '{}'. {} Focus the next action on the most recent operational signal and keep the response ready for human verification.",
			truncate(question, 140),
			knowledge_summary
		),
		suggested_sql,
		pipeline_suggestions,
		ontology_hints,
	}
}

pub fn completion_prompt(
	question: &str,
	dataset_ids: &[Uuid],
	ontology_type_ids: &[Uuid],
	knowledge_hits: &[KnowledgeSearchResult],
) -> String {
	let knowledge = if knowledge_hits.is_empty() {
		"No retrieved knowledge passages.".to_string()
	} else {
		knowledge_hits
			.iter()
			.take(4)
			.map(|hit| format!("- {}: {}", hit.document_title, hit.excerpt))
			.collect::<Vec<_>>()
			.join("\n")
	};

	format!(
		"Answer the operator question with concrete, grounded guidance.\n\nQuestion: {question}\nDatasets: {:?}\nOntology types: {:?}\nKnowledge:\n{knowledge}",
		dataset_ids, ontology_type_ids
	)
}

pub fn apply_live_answer(mut draft: CopilotDraft, live_answer: Option<String>, blocked: bool) -> CopilotDraft {
	if blocked {
		draft.answer =
			"Guardrails blocked this copilot request. Remove unsafe instructions and retry.".to_string();
		draft.suggested_sql = None;
		draft.pipeline_suggestions.clear();
		draft.ontology_hints.clear();
		return draft;
	}

	if let Some(answer) = live_answer.map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
	{
		draft.answer = answer;
	}

	draft
}

#[cfg(test)]
mod tests {
	use super::*;

	fn draft() -> CopilotDraft {
		CopilotDraft {
			answer: "local draft".into(),
			suggested_sql: Some("SELECT 1".into()),
			pipeline_suggestions: vec!["profile source".into()],
			ontology_hints: vec!["use stable ids".into()],
		}
	}

	#[test]
	fn apply_live_answer_prefers_provider_text() {
		let result = apply_live_answer(draft(), Some("  TP53 encodes p53.  ".into()), false);
		assert_eq!(result.answer, "TP53 encodes p53.");
		assert_eq!(result.suggested_sql.as_deref(), Some("SELECT 1"));
	}

	#[test]
	fn apply_live_answer_keeps_draft_when_provider_fails() {
		let result = apply_live_answer(draft(), None, false);
		assert_eq!(result.answer, "local draft");
	}

	#[test]
	fn apply_live_answer_clears_output_when_blocked() {
		let result = apply_live_answer(draft(), Some("should not appear".into()), true);
		assert!(result.answer.contains("Guardrails blocked"));
		assert_eq!(result.suggested_sql, None);
		assert!(result.pipeline_suggestions.is_empty());
	}
}

fn truncate(content: &str, limit: usize) -> String {
	let mut chars = content.chars();
	let truncated = chars.by_ref().take(limit).collect::<String>();
	if chars.next().is_some() {
		format!("{truncated}...")
	} else {
		truncated
	}
}
