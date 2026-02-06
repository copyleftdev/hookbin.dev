mod config;
mod db;
mod error;
mod handlers;
mod models;
mod rate_limit;
mod retention;
mod server;

use clap::Parser;
use config::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        config::Commands::Serve(args) => {
            tracing_subscriber::fmt()
                .with_target(false)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();

            tracing::info!(
                port = args.port,
                data_dir = %args.data.display(),
                "starting hookbin"
            );

            // Placeholder until HB-007 wires up the server
            tracing::info!("hookbin v{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
