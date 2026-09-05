use std::net::SocketAddr;

use mylib_server::{AppState, Config, build_app, features::remote_sources, library_sync};
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow_result::Result<()> {
    let config = Config::load()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&config.log_level))
        .json()
        .init();

    let address = SocketAddr::new(config.host, config.port);
    let state = AppState::initialize(config).await?;
    let scheduler = library_sync::start(state.clone());
    let remote_scheduler = remote_sources::scheduler::start(state.clone());
    let app = build_app(state.clone())?;
    let listener = TcpListener::bind(address).await?;
    info!(%address, version = env!("CARGO_PKG_VERSION"), "MyLib server started");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    scheduler.abort();
    remote_scheduler.abort();
    state.close().await;
    info!("MyLib server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(cause) = tokio::signal::ctrl_c().await {
            error!(%cause, "unable to install Ctrl+C handler");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(cause) => error!(%cause, "unable to install SIGTERM handler"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

mod anyhow_result {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}
