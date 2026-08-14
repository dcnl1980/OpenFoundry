use auth_middleware::begin_tenant_transaction;
use sqlx::PgPool;
use uuid::Uuid;

fn admin_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://openfoundry:openfoundry@127.0.0.1:5432/openfoundry".into())
}

fn test_database_url() -> String {
    std::env::var("AI_TEST_DATABASE_URL").unwrap_or_else(|_| {
        let base = admin_url();
        match base.rsplit_once('/') {
            Some((prefix, _)) => format!("{prefix}/openfoundry_ai_test"),
            None => base,
        }
    })
}

async fn pool() -> PgPool {
    let admin = PgPool::connect(&admin_url()).await.expect("admin connect");
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = 'openfoundry_ai_test')",
    )
    .fetch_one(&admin)
    .await
    .expect("exists");
    if !exists {
        sqlx::query("CREATE DATABASE openfoundry_ai_test")
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
    sqlx::query("GRANT ALL PRIVILEGES ON DATABASE openfoundry_ai_test TO openfoundry_app")
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
async fn tenant_b_cannot_see_tenant_a_conversation_knowledge_or_agent() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let conversation_id = Uuid::now_v7();
    let knowledge_base_id = Uuid::now_v7();
    let agent_id = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO ai_conversations (id, title, messages, tenant_id)
           VALUES ($1, $2, '[]'::jsonb, $3)"#,
    )
    .bind(conversation_id)
    .bind(format!("conv-{}", conversation_id))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert conversation");
    sqlx::query(
        r#"INSERT INTO ai_knowledge_bases (id, name, tenant_id)
           VALUES ($1, $2, $3)"#,
    )
    .bind(knowledge_base_id)
    .bind(format!("kb-{}", knowledge_base_id))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert knowledge base");
    sqlx::query(
        r#"INSERT INTO ai_agents (id, name, tenant_id)
           VALUES ($1, $2, $3)"#,
    )
    .bind(agent_id)
    .bind(format!("agent-{}", agent_id))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert agent");
    tx_a.commit().await.expect("commit");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    let conversation: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM ai_conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(&mut *tx_b)
            .await
            .expect("select conversation");
    let knowledge_base: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM ai_knowledge_bases WHERE id = $1")
            .bind(knowledge_base_id)
            .fetch_optional(&mut *tx_b)
            .await
            .expect("select knowledge base");
    let agent: Option<Uuid> = sqlx::query_scalar("SELECT id FROM ai_agents WHERE id = $1")
        .bind(agent_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select agent");
    assert_eq!(conversation, None);
    assert_eq!(knowledge_base, None);
    assert_eq!(agent, None);
}

#[tokio::test]
async fn tenant_b_cannot_see_tenant_a_provider_or_tool() {
    let pool = pool().await;
    let tenant_a = Uuid::now_v7();
    let tenant_b = Uuid::now_v7();
    let provider_id = Uuid::now_v7();
    let tool_id = Uuid::now_v7();

    let mut tx_a = begin_tenant_transaction(&pool, tenant_a).await.expect("a");
    sqlx::query(
        r#"INSERT INTO ai_providers (
               id, name, provider_type, model_name, endpoint_url, tenant_id
           ) VALUES ($1, $2, 'openai', 'gpt-test', 'https://example.test', $3)"#,
    )
    .bind(provider_id)
    .bind(format!("provider-{}", provider_id))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert provider");
    sqlx::query(
        r#"INSERT INTO ai_tools (id, name, tenant_id) VALUES ($1, $2, $3)"#,
    )
    .bind(tool_id)
    .bind(format!("tool-{}", tool_id))
    .bind(tenant_a)
    .execute(&mut *tx_a)
    .await
    .expect("insert tool");
    tx_a.commit().await.expect("commit");

    let mut tx_b = begin_tenant_transaction(&pool, tenant_b).await.expect("b");
    let provider: Option<Uuid> = sqlx::query_scalar("SELECT id FROM ai_providers WHERE id = $1")
        .bind(provider_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select provider");
    let tool: Option<Uuid> = sqlx::query_scalar("SELECT id FROM ai_tools WHERE id = $1")
        .bind(tool_id)
        .fetch_optional(&mut *tx_b)
        .await
        .expect("select tool");
    let deleted = sqlx::query("DELETE FROM ai_providers WHERE id = $1")
        .bind(provider_id)
        .execute(&mut *tx_b)
        .await
        .expect("delete")
        .rows_affected();
    assert_eq!(provider, None);
    assert_eq!(tool, None);
    assert_eq!(deleted, 0);
}
