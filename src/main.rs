use std::borrow::Cow;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::api::{AppState, router};
use crate::config::Config;
use crate::sync::AtuinSync;

mod agents;
mod api;
mod config;
mod history;
mod history_projection;
mod period;
mod svg;
mod sync;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _sentry = init_sentry();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let config = Config::from_env()?;

    if std::env::args().nth(1).as_deref() == Some("sync-once") {
        let (sync, _, _) = AtuinSync::open(config.atuin_sync).await?;
        let downloaded = sync.sync_once().await?;
        info!(downloaded, "one-shot Atuin sync complete");
        return Ok(());
    }

    let (sync, history, agents) = AtuinSync::open(config.atuin_sync).await?;
    sync.clone().spawn();
    let app = router(AppState {
        history,
        agents,
        cache_ttl_seconds: config.cache_ttl_seconds,
        default_timezone: config.timezone,
    });
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    info!(address = %config.listen, "Atuin Abacus listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

fn init_sentry() -> sentry::ClientInitGuard {
    let dsn: Option<sentry::types::Dsn> = std::env::var("SENTRY_DSN")
        .ok()
        .and_then(|dsn| dsn.parse().ok());
    let environment =
        std::env::var("SENTRY_ENVIRONMENT").unwrap_or_else(|_| "production".to_owned());
    let release = std::env::var("SENTRY_RELEASE").ok().map(Cow::Owned);
    let mut options = sentry::ClientOptions::default();
    options.environment = Some(Cow::Owned(environment));
    options.release = release;
    options.send_default_pii = true;

    sentry::init((dsn, options))
}

async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install Ctrl-C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}
