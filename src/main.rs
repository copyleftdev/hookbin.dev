mod config;
mod db;
mod error;
mod handlers;
mod models;
mod rate_limit;
mod retention;
mod server;

use clap::Parser;
use config::{Cli, Config};

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

            // Placeholder until HB-007 wires up the server
            Ok(())
        }
    }
}
