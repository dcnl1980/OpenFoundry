mod config;
mod domain;
mod handlers;
mod models;

use auth_middleware::jwt::JwtConfig;
use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use query_engine::context::QueryContext;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_config: JwtConfig,
    pub query_ctx: std::sync::Arc<QueryContext>,
    pub distributed_query_workers: usize,
}

impl axum::extract::FromRef<AppState> for JwtConfig {
    fn from_ref(state: &AppState) -> Self {
        state.jwt_config.clone()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = config::AppConfig::from_env().expect("failed to load config");

    let migration_url = auth_middleware::resolve_migration_database_url(&cfg.database_url);
    let migration_pool = sqlx::PgPool::connect(&migration_url)
        .await
        .expect("failed to connect to migration database");
    sqlx::migrate!("./migrations")
        .run(&migration_pool)
        .await
        .expect("failed to run migrations");
    migration_pool.close().await;

    let pool = auth_middleware::connect_runtime_pool(&cfg.database_url)
        .await
        .expect("failed to connect to database");

    let jwt_config = JwtConfig::new(&cfg.jwt_secret);

    let query_ctx = std::sync::Arc::new(QueryContext::new());

    let state = AppState {
        db: pool,
        jwt_config: jwt_config.clone(),
        query_ctx,
        distributed_query_workers: cfg.distributed_query_workers.max(1),
    };

    let public = Router::new()
        .route("/health", get(|| async { "ok" }));

    let protected = Router::new()
        .route("/api/v1/queries/execute", post(handlers::execute::execute_query))
        .route("/api/v1/queries/explain", post(handlers::explain::explain_query))
        .route("/api/v1/queries/saved", post(handlers::saved::create_saved_query))
        .route("/api/v1/queries/saved", get(handlers::saved::list_saved_queries))
        .route("/api/v1/queries/saved/{id}", delete(handlers::saved::delete_saved_query))
        .layer(middleware::from_fn_with_state(
            jwt_config,
            auth_middleware::auth_layer,
        ));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .with_state(state);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    tracing::info!("starting query-service on {addr}");
    service_runtime::serve(app, &addr, service_runtime::TlsSettings::from_env())
        .await
        .expect("server error");
}
