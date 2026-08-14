use uuid::Uuid;

use crate::models::role::Role;

pub fn founding_role_name(
    other_users_in_tenant: i64,
    email: &str,
    bootstrap_admin_email: Option<&str>,
) -> &'static str {
    if bootstrap_admin_matches(email, bootstrap_admin_email) {
        return "admin";
    }
    if other_users_in_tenant == 0 {
        "admin"
    } else {
        "viewer"
    }
}

pub fn bootstrap_admin_matches(email: &str, bootstrap_admin_email: Option<&str>) -> bool {
    bootstrap_admin_email
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|expected| expected.eq_ignore_ascii_case(email.trim()))
}

#[derive(Debug, Clone, Default)]
pub struct AccessBundle {
    pub roles: Vec<String>,
    pub groups: Vec<String>,
    pub permissions: Vec<String>,
}

pub async fn assign_role(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_roles (user_id, role_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(role_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn remove_role(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    role_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role_id = $2")
        .bind(user_id)
        .bind(role_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn get_role_by_name(
    conn: &mut sqlx::PgConnection,
    name: &str,
) -> Result<Option<Role>, sqlx::Error> {
    sqlx::query_as::<_, Role>("SELECT id, name, description, created_at FROM roles WHERE name = $1")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
}

pub async fn get_user_access_bundle(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
) -> Result<AccessBundle, sqlx::Error> {
    let roles = sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT name FROM (
               SELECT r.name AS name
               FROM roles r
               INNER JOIN user_roles ur ON ur.role_id = r.id
               WHERE ur.user_id = $1
               UNION
               SELECT r.name AS name
               FROM roles r
               INNER JOIN group_roles gr ON gr.role_id = r.id
               INNER JOIN group_members gm ON gm.group_id = gr.group_id
               WHERE gm.user_id = $1
           ) effective_roles
           ORDER BY name"#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    let groups = sqlx::query_scalar::<_, String>(
        r#"SELECT g.name
           FROM groups g
           INNER JOIN group_members gm ON gm.group_id = g.id
           WHERE gm.user_id = $1
           ORDER BY g.name"#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    let permissions = sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT p.resource || ':' || p.action AS permission_key
           FROM permissions p
           INNER JOIN role_permissions rp ON rp.permission_id = p.id
           WHERE rp.role_id IN (
               SELECT ur.role_id
               FROM user_roles ur
               WHERE ur.user_id = $1
               UNION
               SELECT gr.role_id
               FROM group_roles gr
               INNER JOIN group_members gm ON gm.group_id = gr.group_id
               WHERE gm.user_id = $1
           )
           ORDER BY permission_key"#,
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(AccessBundle {
        roles,
        groups,
        permissions,
    })
}

pub async fn assign_named_role(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    tenant_id: Uuid,
    role_name: &str,
) -> Result<(), sqlx::Error> {
    let Some(role) = get_role_by_name(conn, role_name).await? else {
        return Ok(());
    };
    sqlx::query(
        r#"INSERT INTO user_roles (user_id, role_id, tenant_id)
           VALUES ($1, $2, $3)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(user_id)
    .bind(role.id)
    .bind(tenant_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn assign_founding_role(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    tenant_id: Uuid,
    email: &str,
    bootstrap_admin_email: Option<&str>,
) -> Result<&'static str, sqlx::Error> {
    let others: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE tenant_id = $1 AND id <> $2",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    let role_name = founding_role_name(others, email, bootstrap_admin_email);
    assign_named_role(conn, user_id, tenant_id, role_name).await?;
    Ok(role_name)
}

pub async fn ensure_bootstrap_admin(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    tenant_id: Uuid,
    email: &str,
    bootstrap_admin_email: Option<&str>,
) -> Result<bool, sqlx::Error> {
    if !bootstrap_admin_matches(email, bootstrap_admin_email) {
        return Ok(false);
    }
    assign_named_role(conn, user_id, tenant_id, "admin").await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_user_in_a_tenant_is_admin() {
        assert_eq!(founding_role_name(0, "ops@example.test", None), "admin");
    }

    #[test]
    fn later_users_in_the_same_tenant_are_viewers() {
        assert_eq!(founding_role_name(1, "ops@example.test", None), "viewer");
    }

    #[test]
    fn bootstrap_email_is_always_admin() {
        assert_eq!(
            founding_role_name(3, "Owner@Example.Test", Some("owner@example.test")),
            "admin"
        );
        assert!(!bootstrap_admin_matches("other@example.test", Some("owner@example.test")));
    }
}
