use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

use crate::claims::Claims;

/// Session GUC read by PostgreSQL RLS policies.
pub const TENANT_SETTING: &str = "openfoundry.tenant_id";

/// Row-level security context derived from user claims.
/// Services use this to scope DB queries to the user's org/permissions.
#[derive(Debug, Clone)]
pub struct RlsContext {
    pub user_id: Uuid,
    pub org_id: Option<Uuid>,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    pub attributes: Value,
}

impl From<&Claims> for RlsContext {
    fn from(claims: &Claims) -> Self {
        Self {
            user_id: claims.sub,
            org_id: claims.org_id,
            roles: claims.roles.clone(),
            permissions: claims.permissions.clone(),
            attributes: claims.attributes.clone(),
        }
    }
}

impl RlsContext {
    /// Tenant partition used for `SET LOCAL openfoundry.tenant_id`.
    pub fn tenant_scope_id(&self) -> Uuid {
        self.org_id.unwrap_or(self.user_id)
    }

    /// Returns true if the user is an admin (RBAC only — does not bypass tenant RLS).
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|r| r == "admin")
    }

    /// Returns true if a permission key is present.
    pub fn has_permission(&self, permission: &str) -> bool {
        self.is_admin() || self.permissions.iter().any(|candidate| candidate == permission)
    }

    /// SQL fragment for filtering by org_id. Admins still stay inside their tenant.
    pub fn org_filter(&self, column: &str) -> String {
        format!("{column} = '{}'", self.tenant_scope_id())
    }

    /// SQL fragment that scopes access to either owner or organization.
    pub fn owner_or_org_filter(&self, owner_column: &str, org_column: &str) -> String {
        if let Some(org) = self.org_id {
            format!("({owner_column} = '{}' OR {org_column} = '{org}')", self.user_id)
        } else {
            format!("{owner_column} = '{}'", self.user_id)
        }
    }
}

/// SQL that applies the tenant GUC for the current transaction (`SET LOCAL`).
pub fn set_tenant_guc_sql() -> &'static str {
    "SELECT set_config($1, $2, true)"
}

pub async fn apply_tenant_guc(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(set_tenant_guc_sql())
        .bind(TENANT_SETTING)
        .bind(tenant_id.to_string())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn begin_tenant_transaction(
    pool: &sqlx::PgPool,
    tenant_id: Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    apply_tenant_guc(&mut tx, tenant_id).await?;
    Ok(tx)
}

/// Row returned by a `SECURITY DEFINER` due-work function.
/// Scheduled workers discover ids this way, then open a tenant transaction.
#[derive(Debug, Clone, FromRow)]
pub struct DueWork {
    pub id: Uuid,
    pub tenant_id: Uuid,
}

pub async fn fetch_due_work(
    pool: &sqlx::PgPool,
    sql: &str,
) -> Result<Vec<DueWork>, sqlx::Error> {
    sqlx::query_as::<_, DueWork>(sql).fetch_all(pool).await
}

/// True when the connected role would silently skip FORCE ROW LEVEL SECURITY.
pub fn is_privileged_runtime_role(rolsuper: bool, rolbypassrls: bool) -> bool {
    rolsuper || rolbypassrls
}

pub fn privileged_runtime_role_message(role: &str) -> String {
    format!(
        "DATABASE_URL must use a non-superuser, non-BYPASSRLS role (got '{role}'). Use openfoundry_app so FORCE ROW LEVEL SECURITY applies."
    )
}

pub fn resolve_migration_database_url(runtime_url: &str) -> String {
    std::env::var("MIGRATION_DATABASE_URL").unwrap_or_else(|_| runtime_url.to_string())
}

pub async fn reject_privileged_runtime_role(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    let (rolsuper, rolbypassrls, role): (bool, bool, String) = sqlx::query_as(
        "SELECT rolsuper, rolbypassrls, current_user FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await?;
    if is_privileged_runtime_role(rolsuper, rolbypassrls) {
        return Err(sqlx::Error::Configuration(
            privileged_runtime_role_message(&role).into(),
        ));
    }
    Ok(())
}

pub async fn connect_runtime_pool(database_url: &str) -> Result<sqlx::PgPool, sqlx::Error> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;
    reject_privileged_runtime_role(&pool).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_guc_name_matches_architecture() {
        assert_eq!(TENANT_SETTING, "openfoundry.tenant_id");
        assert!(set_tenant_guc_sql().contains("set_config"));
    }

    #[test]
    fn rls_context_tenant_scope_matches_claims() {
        let user = Uuid::from_u128(5);
        let org = Uuid::from_u128(9);
        let with_org = RlsContext {
            user_id: user,
            org_id: Some(org),
            roles: vec!["admin".into()],
            permissions: vec![],
            attributes: Value::Null,
        };
        let personal = RlsContext {
            org_id: None,
            ..with_org.clone()
        };
        assert_eq!(with_org.tenant_scope_id(), org);
        assert_eq!(personal.tenant_scope_id(), user);
        assert_eq!(with_org.org_filter("tenant_id"), format!("tenant_id = '{org}'"));
    }

    #[test]
    fn privileged_runtime_role_detects_superuser_and_bypass() {
        assert!(is_privileged_runtime_role(true, false));
        assert!(is_privileged_runtime_role(false, true));
        assert!(is_privileged_runtime_role(true, true));
        assert!(!is_privileged_runtime_role(false, false));
        assert!(privileged_runtime_role_message("openfoundry").contains("openfoundry_app"));
    }

    #[test]
    fn migration_url_falls_back_to_runtime_url() {
        let previous = std::env::var("MIGRATION_DATABASE_URL").ok();
        // SAFETY: this unit test is the only code that mutates the process env here.
        unsafe {
            std::env::remove_var("MIGRATION_DATABASE_URL");
        }
        assert_eq!(
            resolve_migration_database_url("postgres://openfoundry_app@localhost/openfoundry"),
            "postgres://openfoundry_app@localhost/openfoundry"
        );
        unsafe {
            match previous {
                Some(value) => std::env::set_var("MIGRATION_DATABASE_URL", value),
                None => std::env::remove_var("MIGRATION_DATABASE_URL"),
            }
        }
    }
}
