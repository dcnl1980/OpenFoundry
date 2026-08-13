use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// JWT claims embedded in every access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — user ID (UUID v7).
    pub sub: Uuid,
    /// Issued at (epoch seconds).
    pub iat: i64,
    /// Expiration (epoch seconds).
    pub exp: i64,
    /// Token ID — unique per token for revocation.
    pub jti: Uuid,
    /// User email.
    pub email: String,
    /// Display name.
    pub name: String,
    /// Roles assigned to the user.
    pub roles: Vec<String>,
    /// Effective permissions assigned to the subject.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<String>,
    /// Optional organization/tenant scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_id: Option<Uuid>,
    /// Arbitrary subject attributes used by ABAC policies.
    #[serde(default)]
    pub attributes: Value,
    /// Authentication methods used for this session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auth_methods: Vec<String>,
    /// Token intent, e.g. access, refresh, or api_key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_use: Option<String>,
    /// API key identifier when the claims were issued for programmatic access.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_id: Option<Uuid>,
}

impl Claims {
    /// Check if the token has expired.
    pub fn is_expired(&self) -> bool {
        let now = Utc::now().timestamp();
        self.exp < now
    }

    /// Check if the user has a specific role.
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// Check if the user has any of the given roles.
    pub fn has_any_role(&self, roles: &[&str]) -> bool {
        roles.iter().any(|r| self.has_role(r))
    }

    /// Check if the subject has a permission key, e.g. `users:read`.
    pub fn has_permission_key(&self, permission: &str) -> bool {
        let resource_wildcard = permission
            .split_once(':')
            .map(|(resource, _)| format!("{resource}:*"));

        self.has_role("admin")
            || self.permissions.iter().any(|candidate| {
                candidate == permission
                    || candidate == "*:*"
                    || resource_wildcard.as_ref().is_some_and(|wildcard| candidate == wildcard)
            })
    }

    /// Check if the subject can perform an action over a resource.
    pub fn has_permission(&self, resource: &str, action: &str) -> bool {
        self.has_permission_key(&format!("{resource}:{action}"))
    }

    /// Fetch an attribute if present.
    pub fn attribute(&self, key: &str) -> Option<&Value> {
        self.attributes.as_object().and_then(|map| map.get(key))
    }

    /// Stable tenant partition key for row isolation.
    ///
    /// Organization members share `org_id`. Users without an organization are
    /// isolated to their own subject so personal workspaces cannot see each other.
    /// Admin roles do not bypass this boundary.
    pub fn tenant_scope_id(&self) -> Uuid {
        self.org_id.unwrap_or(self.sub)
    }

    /// Get the issued-at time as a DateTime.
    pub fn issued_at(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.iat, 0)
    }

    /// Get the expiration time as a DateTime.
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.exp, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn claims(sub: Uuid, org_id: Option<Uuid>, roles: &[&str]) -> Claims {
        Claims {
            sub,
            iat: 0,
            exp: i64::MAX,
            jti: Uuid::nil(),
            email: "user@example.com".into(),
            name: "User".into(),
            roles: roles.iter().map(|role| (*role).to_string()).collect(),
            permissions: vec![],
            org_id,
            attributes: json!({}),
            auth_methods: vec![],
            token_use: Some("access".into()),
            api_key_id: None,
        }
    }

    #[test]
    fn tenant_scope_uses_organization_when_present() {
        let user = Uuid::from_u128(1);
        let org = Uuid::from_u128(99);
        assert_eq!(claims(user, Some(org), &[]).tenant_scope_id(), org);
    }

    #[test]
    fn tenant_scope_falls_back_to_subject_without_organization() {
        let user = Uuid::from_u128(7);
        assert_eq!(claims(user, None, &[]).tenant_scope_id(), user);
    }

    #[test]
    fn admin_role_does_not_change_tenant_scope() {
        let user = Uuid::from_u128(3);
        let org = Uuid::from_u128(11);
        assert_eq!(claims(user, Some(org), &["admin"]).tenant_scope_id(), org);
        assert_eq!(claims(user, None, &["admin"]).tenant_scope_id(), user);
    }
}
