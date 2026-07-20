mod auth;
mod config;
mod handlers;
mod models;
mod state;

use axum::{
    middleware,
    response::Html,
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use crate::config::AppConfig;
use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = AppConfig::from_env()?;
    let bind_addr = cfg.bind_addr.clone();
    let state = AppState::load(cfg)?;

    let app = Router::new()
        .route("/", get(index_page))
        .route("/login", get(login_page))
        .route("/healthz", get(health))
        .merge(client_routes(state.clone()))
        .merge(admin_routes(state.clone()))
        .with_state(state);

    let addr: SocketAddr = bind_addr
        .parse()
        .map_err(|e| format!("invalid BIND_ADDR `{bind_addr}`: {e}"))?;
    tracing::info!("rustdesk-config-server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn index_page() -> Html<&'static str> {
    Html(include_str!("web/index.html"))
}

async fn login_page() -> Html<&'static str> {
    Html(include_str!("web/login.html"))
}

/// 客户端(patched rustdesk)调用的接口,统一要求 Bearer token。
fn client_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/config", get(handlers::client::get_config))
        .route("/version/latest", post(handlers::client::version_latest))
        .route_layer(middleware::from_fn_with_state(
            state,
            auth::require_client_token,
        ))
}

/// 管理面板:`/admin/login` 公开,`/admin/api/*` 需要登录会话。
fn admin_routes(state: AppState) -> Router<AppState> {
    let protected = Router::new()
        .route("/api/whoami", get(handlers::admin::whoami))
        .route("/api/logout", post(handlers::admin::logout))
        .route("/api/state", get(handlers::admin::get_state))
        .route("/api/server", post(handlers::admin::set_server))
        .route("/api/update", post(handlers::admin::set_update))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_session,
        ));
    let admin = Router::new()
        .route("/login", post(handlers::admin::login))
        .merge(protected);
    Router::new().nest("/admin", admin)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
