use serde_json::Value;

use crate::models::property::CreatePropertyRequest;

#[derive(Debug)]
pub struct PreparedProperty {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub property_type: String,
    pub required: bool,
    pub unique_constraint: bool,
    pub default_value: Option<Value>,
    pub validation_rules: Option<Value>,
}

pub fn prepare_new_property(body: &CreatePropertyRequest) -> Result<PreparedProperty, String> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err("name is required".into());
    }
    validate_property_type(&body.property_type)?;

    Ok(PreparedProperty {
        name: name.to_string(),
        display_name: body
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(name)
            .to_string(),
        description: body.description.clone().unwrap_or_default(),
        property_type: body.property_type.clone(),
        required: body.required.unwrap_or(false),
        unique_constraint: body.unique_constraint.unwrap_or(false),
        default_value: body.default_value.clone(),
        validation_rules: body.validation_rules.clone(),
    })
}

const VALID_TYPES: &[&str] = &[
    "string", "integer", "float", "boolean", "date", "timestamp", "json", "array", "reference",
];

pub fn validate_property_type(property_type: &str) -> Result<(), String> {
    if VALID_TYPES.contains(&property_type) {
        Ok(())
    } else {
        Err(format!(
            "invalid property type '{property_type}', valid types: {VALID_TYPES:?}"
        ))
    }
}

pub fn validate_property_value(property_type: &str, value: &Value) -> Result<(), String> {
    match property_type {
        "string" => {
            if value.is_string() { Ok(()) } else { Err("expected string value".into()) }
        }
        "integer" => {
            if value.is_i64() || value.is_u64() { Ok(()) } else { Err("expected integer value".into()) }
        }
        "float" => {
            if value.is_f64() || value.is_i64() { Ok(()) } else { Err("expected numeric value".into()) }
        }
        "boolean" => {
            if value.is_boolean() { Ok(()) } else { Err("expected boolean value".into()) }
        }
        "json" | "array" => Ok(()),
        "date" | "timestamp" => {
            if value.is_string() { Ok(()) } else { Err("expected string date value".into()) }
        }
        "reference" => {
            if value.is_string() { Ok(()) } else { Err("expected UUID string for reference".into()) }
        }
        _ => Err(format!("unknown type: {property_type}")),
    }
}

pub fn validate_cardinality(cardinality: &str) -> Result<(), String> {
    match cardinality {
        "one_to_one" | "one_to_many" | "many_to_one" | "many_to_many" => Ok(()),
        _ => Err(format!("invalid cardinality '{cardinality}', valid: one_to_one, one_to_many, many_to_one, many_to_many")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_new_property_rejects_blank_name() {
        let err = prepare_new_property(&CreatePropertyRequest {
            name: "   ".into(),
            display_name: None,
            description: None,
            property_type: "string".into(),
            required: None,
            unique_constraint: None,
            default_value: None,
            validation_rules: None,
        })
        .expect_err("blank names must be rejected");

        assert!(err.contains("name"));
    }

    #[test]
    fn prepare_new_property_rejects_unknown_type() {
        let err = prepare_new_property(&CreatePropertyRequest {
            name: "status".into(),
            display_name: None,
            description: None,
            property_type: "uuid".into(),
            required: None,
            unique_constraint: None,
            default_value: None,
            validation_rules: None,
        })
        .expect_err("unknown property types must be rejected");

        assert!(err.contains("invalid property type"));
    }

    #[test]
    fn prepare_new_property_defaults_display_name() {
        let prepared = prepare_new_property(&CreatePropertyRequest {
            name: "status".into(),
            display_name: None,
            description: None,
            property_type: "string".into(),
            required: Some(true),
            unique_constraint: None,
            default_value: None,
            validation_rules: None,
        })
        .expect("valid property should prepare");

        assert_eq!(prepared.name, "status");
        assert_eq!(prepared.display_name, "status");
        assert!(prepared.required);
        assert!(!prepared.unique_constraint);
    }
}
