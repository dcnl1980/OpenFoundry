use auth_middleware::begin_tenant_transaction;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("AUDIT_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_audit_test"),
            None => base,
        }
    })
}

async fn pool() -> PgPool {
    let admin = PgPool::connect(&admin_url()).await.expect("admin connect");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_audit_test')",
    )
    .fetch_one(&admin)
    .await
    .expect("exists");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_audit_test")
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
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_audit_test TO openfoundry_app")
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
async fn tenant_b_cannot_see_tenant_a_audit_event_and_sequences_are_per_tenant() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let event_a = Uuid::now_v7();
    let event_b = Uuid::now_v7();
    let policy_a = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO audit_events (
               id, sequence, previous_hash, entry_hash, source_service, channel, actor, action,
               resource_type, resource_id, status, severity, classification, metadata, labels,
               retention_until, occurred_at, ingested_at, tenant_id
           ) VALUES (
               $1, 1, 'GENESIS', 'hash-a', 'auth-service', 'http', 'user:a', 'login',
               'session', 's1', 'success', 'low', 'public', '{}'::jsonb, '[]'::jsonb,
               NOW() + interval '30 days', NOW(), NOW(), $2
           )"#,
    )
    .bind(event_a)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert event a");
    sqlx::query(
        r#"INSERT INTO audit_policies (
               id, name, description, scope, classification, retention_days, legal_hold,
               purge_mode, active, rules, updated_by, tenant_id
           ) VALUES (
               $1, $2, '', 'ops', 'public', 30, false, 'retain', true, '[]'::jsonb, 'a', $3
           )"#,
    )
    .bind(policy_a)
    .bind(format!("policy-{policy_a}"))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert policy");
    tx_a.commit().await.expect("commit a");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    sqlx::query(
        r#"INSERT INTO audit_events (
               id, sequence, previous_hash, entry_hash, source_service, channel, actor, action,
               resource_type, resource_id, status, severity, classification, metadata, labels,
               retention_until, occurred_at, ingested_at, tenant_id
           ) VALUES (
               $1, 1, 'GENESIS', 'hash-b', 'auth-service', 'http', 'user:b', 'login',
               'session', 's2', 'success', 'low', 'public', '{}'::jsonb, '[]'::jsonb,
               NOW() + interval '30 days', NOW(), NOW(), $2
           )"#,
    )
    .bind(event_b)
    .bind(tenant_b)
    .execute(&mut *tx_b)
    .await
    .expect("insert event b sequence 1");
    let visible: Option<Uuid> = sqlx::query_scalar("SELECT id FROM audit_events WHERE id = $1")
        .bind(event_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select event");
    let policy: Option<Uuid> = sqlx::query_scalar("SELECT id FROM audit_policies WHERE id = $1")
        .bind(policy_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select policy");
    let deleted = sqlx::query("DELETE FROM audit_events WHERE id = $1")
        .bind(event_a)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    assert_eq!(visible, None);
    assert_eq!(policy, None);
    assert_eq!(deleted, 0);
}
