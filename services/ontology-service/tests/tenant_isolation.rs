use std::collections::HashSet;

use auth_middleware::{begin_tenant_transaction, apply_tenant_guc, TENANT_SETTING};
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("ONTOLOGY_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_ontology_test"),
            None => base,
        }
    })
}

async fn ensure_test_database() {
    let admin = PgPool::connect(&admin_url())
        .await
        .expect("connect to postgres");
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_ontology_test')")
        .fetch_one(&admin)
        .await
        .expect("check test database");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_ontology_test")
            .execute(&admin)
            .await
            .expect("create ontology test database");
    }
}

fn app_database_url() -> String {
    test_database_url().replacen("://openfoundry:", "://openfoundry_app:", 1)
}

async fn ensure_app_role(admin: &PgPool) {
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
    .execute(admin)
    .await
    .expect("create app role");
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_ontology_test TO openfoundry_app")
        .execute(admin)
        .await
        .ok();
}

async fn pool() -> PgPool {
    ensure_test_database().await;
    let admin = PgPool::connect(&test_database_url())
        .await
        .expect("connect to ontology test database as owner");
    sqlx::migrate!("./migrations")
        .run(&admin)
        .await
        .expect("run ontology migrations");
    ensure_app_role(&admin).await;
    sqlx::query("GRANT ALL ON SCHEMA public TO openfoundry_app")
        .execute(&admin)
        .await
        .expect("grant schema");
    sqlx::query("GRANT ALL ON ALL TABLES IN SCHEMA public TO openfoundry_app")
        .execute(&admin)
        .await
        .expect("grant tables");
    sqlx::query("GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO openfoundry_app")
        .execute(&admin)
        .await
        .expect("grant sequences");
    sqlx::query("GRANT EXECUTE ON ALL FUNCTIONS IN SCHEMA public TO openfoundry_app")
        .execute(&admin)
        .await
        .expect("grant functions");

    PgPool::connect(&app_database_url())
        .await
        .expect("connect as non-superuser app role")
}

async fn insert_object_type(pool: &PgPool, tenant_id: Uuid, name: &str) -> Uuid {
    let mut tx = begin_tenant_transaction(pool, tenant_id)
        .await
        .expect("begin tenant tx");
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO object_types (id, name, display_name, description, owner_id, tenant_id)
           VALUES ($1, $2, $3, '', $4, $4)"#,
    )
    .bind(id)
    .bind(name)
    .bind(name)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .expect("insert object type");
    tx.commit().await.expect("commit");
    id
}

async fn insert_object(pool: &PgPool, tenant_id: Uuid, type_id: Uuid) -> Uuid {
    let mut tx = begin_tenant_transaction(pool, tenant_id)
        .await
        .expect("begin tenant tx");
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO object_instances (id, object_type_id, properties, created_by, tenant_id)
           VALUES ($1, $2, '{}'::jsonb, $3, $3)"#,
    )
    .bind(id)
    .bind(type_id)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .expect("insert object");
    tx.commit().await.expect("commit");
    id
}

async fn insert_link_type(pool: &PgPool, tenant_id: Uuid, source: Uuid, target: Uuid) -> Uuid {
    let mut tx = begin_tenant_transaction(pool, tenant_id)
        .await
        .expect("begin tenant tx");
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO link_types (id, name, display_name, description, source_type_id, target_type_id, owner_id, tenant_id)
           VALUES ($1, 'related', 'Related', '', $2, $3, $4, $4)"#,
    )
    .bind(id)
    .bind(source)
    .bind(target)
    .bind(tenant_id)
    .execute(&mut *tx)
    .await
    .expect("insert link type");
    tx.commit().await.expect("commit");
    id
}

#[tokio::test]
async fn tenants_can_reuse_the_same_object_type_name() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let name = format!("asset-{}", Uuid::now_v7());

    let _ = insert_object_type(&pool, tenant_a, &name).await;
    let _ = insert_object_type(&pool, tenant_b, &name).await;
}

#[tokio::test]
async fn tenant_b_cannot_read_or_search_tenant_a_object_type() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let name = format!("secret-{}", Uuid::now_v7());
    let type_id = insert_object_type(&pool, tenant_a, &name).await;

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b)
        .await
        .expect("tenant b tx");
    let visible: Option<Uuid> = sqlx::query_scalar("SELECT id FROM object_types WHERE id = $1")
        .bind(type_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select by id");
    assert_eq!(visible, None, "guessed UUID must be invisible");

    let search: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM object_types WHERE name ILIKE $1 OR display_name ILIKE $1",
    )
    .bind(format!("%{name}%"))
    .fetch_all(&mut *tx_b)
    .await
    .expect("search");
    assert!(search.is_empty(), "search must not leak tenant A metadata");
}

#[tokio::test]
async fn tenant_b_cannot_read_tenant_a_object_or_link() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let type_id = insert_object_type(&pool, tenant_a, &format!("ot-{}", Uuid::now_v7())).await;
    let object_id = insert_object(&pool, tenant_a, type_id).await;
    let link_type_id = insert_link_type(&pool, tenant_a, type_id, type_id).await;

    let source = object_id;
    let target = insert_object(&pool, tenant_a, type_id).await;

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a)
        .await
        .expect("tenant a tx");
    let link_id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO link_instances (id, link_type_id, source_object_id, target_object_id, created_by, tenant_id)
           VALUES ($1, $2, $3, $4, $5, $5)"#,
    )
    .bind(link_id)
    .bind(link_type_id)
    .bind(source)
    .bind(target)
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert link");
    tx_a.commit().await.expect("commit link");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b)
        .await
        .expect("tenant b tx");
    let object: Option<Uuid> = sqlx::query_scalar("SELECT id FROM object_instances WHERE id = $1")
        .bind(object_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("object lookup");
    let link: Option<Uuid> = sqlx::query_scalar("SELECT id FROM link_instances WHERE id = $1")
        .bind(link_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("link lookup");
    let link_type: Option<Uuid> = sqlx::query_scalar("SELECT id FROM link_types WHERE id = $1")
        .bind(link_type_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("link type lookup");
    assert_eq!(object, None);
    assert_eq!(link, None);
    assert_eq!(link_type, None);
}

#[tokio::test]
async fn unscoped_connection_sees_no_tenant_rows() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let _ = insert_object_type(&pool, tenant_a, &format!("hidden-{}", Uuid::now_v7())).await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM object_types")
        .fetch_one(&pool)
        .await
        .expect("unscoped count");
    assert_eq!(count, 0, "RLS must hide rows when the tenant GUC is unset");
}

#[tokio::test]
async fn tenant_guc_is_local_to_the_transaction() {
    let pool = pool().await;
    let tenant = Uuid::now_v7();
    let mut tx = pool.begin().await.expect("begin");
    apply_tenant_guc(&mut tx, tenant).await.expect("set guc");
    let inside: String = sqlx::query_scalar("SELECT current_setting($1, true)")
        .bind(TENANT_SETTING)
        .fetch_one(&mut *tx)
        .await
        .expect("read guc");
    assert_eq!(inside, tenant.to_string());
    tx.commit().await.expect("commit");

    let outside: Option<String> = sqlx::query_scalar("SELECT current_setting($1, true)")
        .bind(TENANT_SETTING)
        .fetch_one(&pool)
        .await
        .expect("read guc after commit");
    assert!(
        outside.as_deref().unwrap_or("").is_empty(),
        "SET LOCAL must not leak across connections"
    );
}

#[tokio::test]
async fn tenant_a_update_and_delete_cannot_touch_tenant_b() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let type_b = insert_object_type(&pool, tenant_b, &format!("b-{}", Uuid::now_v7())).await;

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a)
        .await
        .expect("tenant a tx");
    let updated = sqlx::query("UPDATE object_types SET display_name = 'hacked' WHERE id = $1")
        .bind(type_b)
        .execute(&mut *tx_a)
        .await
        .expect("update")
        .rows_affected();
    let deleted = sqlx::query("DELETE FROM object_types WHERE id = $1")
        .bind(type_b)
        .execute(&mut *tx_a)
        .await
        .expect("delete")
        .rows_affected();
    assert_eq!(updated, 0);
    assert_eq!(deleted, 0);
    tx_a.commit().await.ok();

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b)
        .await
        .expect("tenant b tx");
    let name: String = sqlx::query_scalar("SELECT display_name FROM object_types WHERE id = $1")
        .bind(type_b)
        .fetch_one(&mut *tx_b)
        .await
        .expect("type still owned by B");
    assert_ne!(name, "hacked");
}

#[tokio::test]
async fn list_for_tenant_a_never_includes_tenant_b_ids() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let a = insert_object_type(&pool, tenant_a, &format!("a-{}", Uuid::now_v7())).await;
    let b = insert_object_type(&pool, tenant_b, &format!("b-{}", Uuid::now_v7())).await;

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a)
        .await
        .expect("tenant a tx");
    let ids: HashSet<Uuid> = sqlx::query_scalar("SELECT id FROM object_types")
        .fetch_all(&mut *tx_a)
        .await
        .expect("list")
        .into_iter()
        .collect();
    assert!(ids.contains(&a));
    assert!(!ids.contains(&b));
}
