mod config;
mod domain;
mod handlers;
mod models;

use auth_middleware::jwt::JwtConfig;
use axum::{
    extract::FromRef,
    middleware,
    routing::{get, post},
    Router,
};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_config: JwtConfig,
}

impl FromRef<AppState> for JwtConfig {
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
    let state = AppState {
        db: pool,
        jwt_config: jwt_config.clone(),
    };

    let public = Router::new().route("/health", get(|| async { "ok" }));

    let protected = Router::new()
        .route(
            "/api/v1/streaming/overview",
            get(handlers::topologies::get_overview),
        )
        .route(
            "/api/v1/streaming/streams",
            get(handlers::streams::list_streams).post(handlers::streams::create_stream),
        )
        .route(
            "/api/v1/streaming/streams/{id}",
            axum::routing::patch(handlers::streams::update_stream),
        )
        .route(
            "/api/v1/streaming/windows",
            get(handlers::streams::list_windows).post(handlers::streams::create_window),
        )
        .route(
            "/api/v1/streaming/windows/{id}",
            axum::routing::patch(handlers::streams::update_window),
        )
        .route(
            "/api/v1/streaming/topologies",
            get(handlers::topologies::list_topologies)
                .post(handlers::topologies::create_topology),
        )
        .route(
            "/api/v1/streaming/topologies/{id}",
            axum::routing::patch(handlers::topologies::update_topology),
        )
        .route(
            "/api/v1/streaming/topologies/{id}/run",
            post(handlers::topologies::run_topology),
        )
        .route(
            "/api/v1/streaming/topologies/{id}/runtime",
            get(handlers::topologies::get_runtime),
        )
        .route(
            "/api/v1/streaming/connectors",
            get(handlers::topologies::list_connectors),
        )
        .route(
            "/api/v1/streaming/live-tail",
            get(handlers::topologies::get_live_tail),
        )
        .layer(middleware::from_fn_with_state(
            jwt_config,
            auth_middleware::auth_layer,
        ));

    let app = Router::new().merge(public).merge(protected).with_state(state);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    tracing::info!("starting streaming-service on {addr}");
    service_runtime::serve(app, &addr, service_runtime::TlsSettings::from_env())
        .await
        .expect("server error");
}
