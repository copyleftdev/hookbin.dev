mod config;
mod db;
mod error;
mod handlers;
mod models;
mod rate_limit;
mod retention;
mod server;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use config::{Cli, Config};
use db::Database;
use server::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        config::Commands::Serve(ref args) => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();

            let config = match Config::load(args) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            tracing::info!(
                port = config.port,
                data_dir = %config.data.display(),
                max_hooks = config.max_hooks,
                max_payload = config.max_payload,
                retention_secs = config.retention,
                rate_limit = config.rate_limit,
                max_requests = config.max_requests,
                "starting hookbin v{}",
                env!("CARGO_PKG_VERSION")
            );

            let db = match Database::open(&config.data) {
                Ok(db) => db,
                Err(e) => {
                    tracing::error!(error = %e, "failed to open database");
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };

            let state = AppState {
                db: Arc::new(db),
                config: Arc::new(config.clone()),
                start_time: Instant::now(),
            };

            let app = server::build_router(state);

            let addr = format!("0.0.0.0:{}", config.port);
            let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
                tracing::error!(addr = %addr, error = %e, "failed to bind");
                e
            })?;

            tracing::info!(addr = %addr, "listening");

            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown_signal())
            .await?;

            tracing::info!("shutdown complete");
            Ok(())
        }
    }
}

/// Wait for Ctrl+C or SIGTERM for graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
