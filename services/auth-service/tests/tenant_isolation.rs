use auth_middleware::begin_tenant_transaction;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("AUTH_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_auth_test"),
            None => base,
        }
    })
}

async fn pool() -> PgPool {
    let admin = PgPool::connect(&admin_url()).await.expect("admin connect");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_auth_test')",
    )
    .fetch_one(&admin)
    .await
    .expect("exists");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_auth_test")
            .execute(&admin)
            .await
            .expect("create db");
    }
    let owner = PgPool::connect(&test_database_url())
        .await
        .expect("owner connect");
    sqlx::migrate!("./migrations")
        .run(&owner)
        .await
        .expect("migrate");
    sqlx::query(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'openfoundry_app') THEN
                CREATE ROLE openfoundry_app LOGIN PASSWORD 'openfoundry' NOSUPERUSER NOBYPASSRLS;
            END IF;
        END
        $$;
        "#,
    )
    .execute(&owner)
    .await
    .ok();
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_auth_test TO openfoundry_app")
        .execute(&admin)
        .await
        .ok();
    sqlx::query("GRANT ALL ON SCHEMA public TO openfoundry_app")
        .execute(&owner)
        .await
        .ok();
    sqlx::query("GRANT ALL ON ALL TABLES IN SCHEMA public TO openfoundry_app")
        .execute(&owner)
        .await
        .ok();
    sqlx::query("GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO openfoundry_app")
        .execute(&owner)
        .await
        .ok();
    PgPool::connect(&test_database_url().replacen("://openfoundry:", "://openfoundry_app:", 1))
        .await
        .expect("app connect")
}

#[tokio::test]
async fn tenant_b_cannot_see_tenant_a_users_keys_or_groups() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let user_a = tenant_a;
    let group_a = Uuid::now_v7();
    let key_a = Uuid::now_v7();
    let email = format!("user-{user_a}@example.test");

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO users (id, email, name, password_hash, tenant_id)
           VALUES ($1, $2, 'Tenant A', 'hash', $3)"#,
    )
    .bind(user_a)
    .bind(&email)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert user");
    sqlx::query(
        r#"INSERT INTO groups (id, name, tenant_id) VALUES ($1, $2, $3)"#,
    )
    .bind(group_a)
    .bind(format!("ops-{group_a}"))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert group");
    sqlx::query(
        r#"INSERT INTO api_keys (id, user_id, name, prefix, tenant_id)
           VALUES ($1, $2, 'deploy', $3, $4)"#,
    )
    .bind(key_a)
    .bind(user_a)
    .bind(format!("ofk_{}", &key_a.to_string()[..8]))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert api key");
    tx_a.commit().await.expect("commit");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    let user: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1")
        .bind(user_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select user");
    let group: Option<Uuid> = sqlx::query_scalar("SELECT id FROM groups WHERE id = $1")
        .bind(group_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select group");
    let key: Option<Uuid> = sqlx::query_scalar("SELECT id FROM api_keys WHERE id = $1")
        .bind(key_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select key");
    let deleted = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_a)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    let viewer: Option<String> = sqlx::query_scalar("SELECT name FROM roles WHERE name = 'viewer'")
        .fetch_optional(&mut *tx_b)
        .await
        .expect("system role");
    assert_eq!(user, None);
    assert_eq!(group, None);
    assert_eq!(key, None);
    assert_eq!(deleted, 0);
    assert_eq!(viewer.as_deref(), Some("viewer"));
}

#[tokio::test]
async fn email_lookup_finds_user_then_tenant_scope_hides_them() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let email = format!("login-{tenant_a}@example.test");

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO users (id, email, name, password_hash, tenant_id)
           VALUES ($1, $2, 'Login', 'hash', $3)"#,
    )
    .bind(tenant_a)
    .bind(&email)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert user");
    tx_a.commit().await.expect("commit");

    let lookup: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT id, tenant_id FROM openfoundry_lookup_user_by_email($1)")
            .bind(&email)
            .fetch_optional(&pool)
            .await
            .expect("security definer lookup");
    assert_eq!(lookup, Some((tenant_a, tenant_a)));

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    let hidden: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(&email)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("rls hide");
    assert_eq!(hidden, None);
}
