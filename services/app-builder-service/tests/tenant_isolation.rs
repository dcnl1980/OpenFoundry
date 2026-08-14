use auth_middleware::begin_tenant_transaction;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("APPBUILDER_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_appbuilder_test"),
            None => base,
        }
    })
}

async fn pool() -> PgPool {
    let admin = PgPool::connect(&admin_url()).await.expect("admin connect");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_appbuilder_test')",
    )
    .fetch_one(&admin)
    .await
    .expect("exists");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_appbuilder_test")
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
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_appbuilder_test TO openfoundry_app")
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
async fn tenant_b_cannot_see_tenant_a_app_and_both_can_use_home_slug() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let app_a = Uuid::now_v7();
    let app_b = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO apps (id, name, slug, description, status, pages, theme, settings, tenant_id)
           VALUES ($1, 'Home', 'home', '', 'draft', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb, $2)"#,
    )
    .bind(app_a)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert a");
    tx_a.commit().await.expect("commit a");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    sqlx::query(
        r#"INSERT INTO apps (id, name, slug, description, status, pages, theme, settings, tenant_id)
           VALUES ($1, 'Home', 'home', '', 'draft', '[]'::jsonb, '{}'::jsonb, '{}'::jsonb, $2)"#,
    )
    .bind(app_b)
    .bind(tenant_b)
    .execute(&mut *tx_b)
    .await
    .expect("insert b home slug");
    let visible: Option<Uuid> = sqlx::query_scalar("SELECT id FROM apps WHERE id = $1")
        .bind(app_a)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select");
    let deleted = sqlx::query("DELETE FROM apps WHERE id = $1")
        .bind(app_a)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    let own: Option<Uuid> = sqlx::query_scalar("SELECT id FROM apps WHERE slug = 'home'")
        .fetch_optional(&mut *tx_b)
        .await
        .expect("own slug");
    assert_eq!(visible, None);
    assert_eq!(deleted, 0);
    assert_eq!(own, Some(app_b));
}

#[tokio::test]
async fn tenant_b_cannot_see_tenant_a_app_template() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let template_id = Uuid::now_v7();
    let key = format!("ops-{}", template_id);

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO app_templates (id, key, name, definition, tenant_id)
           VALUES ($1, $2, 'Ops', '{}'::jsonb, $3)"#,
    )
    .bind(template_id)
    .bind(&key)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert template");
    tx_a.commit().await.expect("commit");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    sqlx::query(
        r#"INSERT INTO app_templates (id, key, name, definition, tenant_id)
           VALUES ($1, $2, 'Ops', '{}'::jsonb, $3)"#,
    )
    .bind(Uuid::now_v7())
    .bind(&key)
    .bind(tenant_b)
    .execute(&mut *tx_b)
    .await
    .expect("insert same key for tenant b");
    let visible: Option<Uuid> = sqlx::query_scalar("SELECT id FROM app_templates WHERE id = $1")
        .bind(template_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select");
    let deleted = sqlx::query("DELETE FROM app_templates WHERE id = $1")
        .bind(template_id)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    assert_eq!(visible, None);
    assert_eq!(deleted, 0);
}

#[tokio::test]
async fn empty_tenant_receives_its_own_clone_of_system_templates() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query("SELECT openfoundry_clone_system_app_templates()")
        .execute(&mut *tx_a)
        .await
        .expect("clone a");
    let keys_a: Vec<String> = sqlx::query_scalar("SELECT key FROM app_templates ORDER BY key")
        .fetch_all(&mut *tx_a)
        .await
        .expect("keys a");
    let ids_a: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM app_templates")
        .fetch_all(&mut *tx_a)
        .await
        .expect("ids a");
    tx_a.commit().await.expect("commit a");
    assert!(
        keys_a.iter().any(|key| key == "ops-center"),
        "expected system template clone, got {keys_a:?}"
    );

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    sqlx::query("SELECT openfoundry_clone_system_app_templates()")
        .execute(&mut *tx_b)
        .await
        .expect("clone b");
    let ids_b: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM app_templates")
        .fetch_all(&mut *tx_b)
        .await
        .expect("ids b");
    assert!(
        !ids_b.iter().any(|id| ids_a.contains(id)),
        "tenant B must not see tenant A's cloned template ids"
    );
    tx_b.commit().await.expect("commit b");
}
