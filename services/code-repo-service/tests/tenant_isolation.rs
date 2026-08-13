use auth_middleware::begin_tenant_transaction;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("CODE_REPO_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_coderepo_test"),
            None => base,
        }
    })
}

async fn pool() -> PgPool {
    let admin = PgPool::connect(&admin_url()).await.expect("admin connect");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_coderepo_test')",
    )
    .fetch_one(&admin)
    .await
    .expect("exists");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_coderepo_test")
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
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_coderepo_test TO openfoundry_app")
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
async fn tenant_b_cannot_see_tenant_a_repository() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let repository_id = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO code_repositories (
               id, name, slug, description, owner, default_branch, visibility,
               object_store_backend, package_kind, tenant_id
           )
           VALUES ($1, $2, $3, '', 'owner', 'main', 'private', 'gitoxide-pack', 'widget', $4)"#,
    )
    .bind(repository_id)
    .bind(format!("repo-{repository_id}"))
    .bind(format!("slug-{repository_id}"))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert");
    tx_a.commit().await.expect("commit");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    let visible: Option<Uuid> = sqlx::query_scalar("SELECT id FROM code_repositories WHERE id = $1")
        .bind(repository_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select");
    let deleted = sqlx::query("DELETE FROM code_repositories WHERE id = $1")
        .bind(repository_id)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    assert_eq!(visible, None);
    assert_eq!(deleted, 0);
}
