use clap::Parser;
use std::path::PathBuf;

/// Single-binary webhook inbox. Accept HTTP, store it, show it.
#[derive(Debug, Parser)]
#[command(name = "hookbin", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Start the hookbin server
    Serve(ServeArgs),
}

#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(long, default_value = "3000")]
    pub port: u16,

    /// Data directory for SQLite database
    #[arg(long, default_value = "./hookbin-data")]
    pub data: PathBuf,

    /// Maximum number of hooks
    #[arg(long, default_value = "100")]
    pub max_hooks: u32,

    /// Maximum payload size in bytes
    #[arg(long, default_value = "1048576")]
    pub max_payload: usize,

    /// Request retention in seconds
    #[arg(long, default_value = "86400")]
    pub retention: u64,

    /// Rate limit per hook (requests per minute)
    #[arg(long, default_value = "60")]
    pub rate_limit: u32,

    /// Maximum stored requests per hook
    #[arg(long, default_value = "1000")]
    pub max_requests: u32,

    /// Optional TOML config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}
