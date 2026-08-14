mod config;
mod domain;
mod handlers;
mod models;

use auth_middleware::jwt::JwtConfig;
use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};
use query_engine::context::QueryContext;
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_config: JwtConfig,
    pub query_ctx: std::sync::Arc<QueryContext>,
    pub distributed_pipeline_workers: usize,
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
        distributed_pipeline_workers: cfg.distributed_pipeline_workers.max(1),
    };

    let scheduler_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Err(error) = domain::executor::run_due_scheduled_pipelines(&scheduler_state).await {
                tracing::warn!("pipeline scheduling evaluation failed: {error}");
            }
        }
    });

    let public = Router::new()
        .route("/health", get(|| async { "ok" }));

    let protected = Router::new()
        // Pipeline CRUD
        .route("/api/v1/pipelines", post(handlers::crud::create_pipeline))
        .route("/api/v1/pipelines", get(handlers::crud::list_pipelines))
        .route("/api/v1/pipelines/{id}", get(handlers::crud::get_pipeline))
        .route("/api/v1/pipelines/{id}", put(handlers::crud::update_pipeline))
        .route("/api/v1/pipelines/{id}", delete(handlers::crud::delete_pipeline))
        // Execution
        .route("/api/v1/pipelines/{id}/run", post(handlers::execute::trigger_run))
        .route(
            "/api/v1/pipelines/triggers/cron/run-due",
            post(handlers::execute::run_due_scheduled_pipelines),
        )
        // Runs
        .route("/api/v1/pipelines/{id}/runs", get(handlers::runs::list_runs))
        .route("/api/v1/pipelines/{pipeline_id}/runs/{run_id}", get(handlers::runs::get_run))
        .route(
            "/api/v1/pipelines/{pipeline_id}/runs/{run_id}/retry",
            post(handlers::execute::retry_run),
        )
        // Lineage
        .route("/api/v1/lineage/datasets/{dataset_id}", get(handlers::lineage::get_dataset_lineage))
        .route(
            "/api/v1/lineage/datasets/{dataset_id}/columns",
            get(handlers::lineage::get_dataset_column_lineage),
        )
        .route("/api/v1/lineage", get(handlers::lineage::get_full_lineage))
        .layer(middleware::from_fn_with_state(
            jwt_config,
            auth_middleware::auth_layer,
        ));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .with_state(state);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    tracing::info!("starting pipeline-service on {addr}");
    service_runtime::serve(app, &addr, service_runtime::TlsSettings::from_env())
        .await
        .expect("server error");
}

