use axum::middleware as axum_mw;
use axum::Router;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use devfs::config::Config;
use devfs::dispatcher::dispatch;
use devfs::middleware::s3_operation_middleware;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::load();
    let addr = config.addr();
    let state = config.into_app_state();

    let app = Router::new()
        .fallback(dispatch)
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            s3_operation_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    tracing::info!("devfs listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
