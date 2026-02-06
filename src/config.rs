use std::path::{Path, PathBuf};

/// Resolved configuration with all values set (defaults → TOML → CLI).
///
/// This is the type the rest of the codebase uses. Constructed via `Config::load`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub data: PathBuf,
    pub max_hooks: u32,
    pub max_payload: usize,
    pub retention: u64,
    pub rate_limit: u32,
    pub max_requests: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3000,
            data: PathBuf::from("./hookbin-data"),
            max_hooks: 100,
            max_payload: 1_048_576,
            retention: 86_400,
            rate_limit: 60,
            max_requests: 1000,
        }
    }
}

impl Config {
    /// Load config by merging: defaults → TOML file → CLI overrides.
    ///
    /// CLI flags always win over TOML values, which win over defaults.
    pub fn load(args: &ServeArgs) -> Result<Self, crate::error::AppError> {
        let mut config = Config::default();

        // Layer 2: TOML file (if specified)
        if let Some(ref path) = args.config {
            let file_config = FileConfig::load(path)?;
            file_config.apply_to(&mut config);
        }

        // Layer 3: CLI overrides (only set values that were explicitly passed)
        args.apply_to(&mut config);

        Ok(config)
    }
}

// -- CLI layer (clap) --

/// Top-level CLI parsed by clap.
#[derive(Debug, clap::Parser)]
#[command(
    name = "hookbin",
    version,
    about = "Single-binary webhook inbox. Accept HTTP, store it, show it."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Start the hookbin server
    Serve(ServeArgs),
}

/// CLI arguments for the `serve` subcommand.
///
/// All fields are `Option` so we can distinguish "user set this" from "default".
/// Defaults are applied in `Config::default()`, not here.
#[derive(Debug, clap::Args)]
pub struct ServeArgs {
    /// Port to listen on [default: 3000]
    #[arg(long)]
    pub port: Option<u16>,

    /// Data directory for SQLite database [default: ./hookbin-data]
    #[arg(long)]
    pub data: Option<PathBuf>,

    /// Maximum number of hooks [default: 100]
    #[arg(long)]
    pub max_hooks: Option<u32>,

    /// Maximum payload size in bytes [default: 1048576]
    #[arg(long)]
    pub max_payload: Option<usize>,

    /// Request retention in seconds [default: 86400]
    #[arg(long)]
    pub retention: Option<u64>,

    /// Rate limit per hook (requests per minute) [default: 60]
    #[arg(long)]
    pub rate_limit: Option<u32>,

    /// Maximum stored requests per hook [default: 1000]
    #[arg(long)]
    pub max_requests: Option<u32>,

    /// Path to TOML config file
    #[arg(long)]
    pub config: Option<PathBuf>,
}

impl ServeArgs {
    /// Apply explicitly-set CLI flags onto an existing config.
    fn apply_to(&self, config: &mut Config) {
        if let Some(port) = self.port {
            config.port = port;
        }
        if let Some(ref data) = self.data {
            config.data = data.clone();
        }
        if let Some(max_hooks) = self.max_hooks {
            config.max_hooks = max_hooks;
        }
        if let Some(max_payload) = self.max_payload {
            config.max_payload = max_payload;
        }
        if let Some(retention) = self.retention {
            config.retention = retention;
        }
        if let Some(rate_limit) = self.rate_limit {
            config.rate_limit = rate_limit;
        }
        if let Some(max_requests) = self.max_requests {
            config.max_requests = max_requests;
        }
    }
}

// -- TOML file layer --

/// TOML config file representation. All fields optional.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    port: Option<u16>,
    data: Option<PathBuf>,
    max_hooks: Option<u32>,
    max_payload: Option<usize>,
    retention: Option<u64>,
    rate_limit: Option<u32>,
    max_requests: Option<u32>,
}

impl FileConfig {
    fn load(path: &Path) -> Result<Self, crate::error::AppError> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            crate::error::AppError::InvalidInput(format!(
                "cannot read config file '{}': {}",
                path.display(),
                e
            ))
        })?;

        toml::from_str(&contents).map_err(|e| {
            crate::error::AppError::InvalidInput(format!(
                "invalid TOML in '{}': {}",
                path.display(),
                e
            ))
        })
    }

    fn apply_to(&self, config: &mut Config) {
        if let Some(port) = self.port {
            config.port = port;
        }
        if let Some(ref data) = self.data {
            config.data = data.clone();
        }
        if let Some(max_hooks) = self.max_hooks {
            config.max_hooks = max_hooks;
        }
        if let Some(max_payload) = self.max_payload {
            config.max_payload = max_payload;
        }
        if let Some(retention) = self.retention {
            config.retention = retention;
        }
        if let Some(rate_limit) = self.rate_limit {
            config.rate_limit = rate_limit;
        }
        if let Some(max_requests) = self.max_requests {
            config.max_requests = max_requests;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // AC-1: Default values
    #[test]
    fn defaults_are_correct() {
        let config = Config::default();
        assert_eq!(config.port, 3000);
        assert_eq!(config.data, PathBuf::from("./hookbin-data"));
        assert_eq!(config.max_hooks, 100);
        assert_eq!(config.max_payload, 1_048_576);
        assert_eq!(config.retention, 86_400);
        assert_eq!(config.rate_limit, 60);
        assert_eq!(config.max_requests, 1000);
    }

    // AC-1: No flags, no TOML → all defaults
    #[test]
    fn load_with_no_args_no_toml_uses_defaults() {
        let args = ServeArgs {
            port: None,
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: None,
        };
        let config = Config::load(&args).unwrap();
        assert_eq!(config, Config::default());
    }

    // AC-2: CLI flags override defaults
    #[test]
    fn cli_flags_override_defaults() {
        let args = ServeArgs {
            port: Some(9090),
            data: Some(PathBuf::from("/tmp/hb")),
            max_hooks: Some(50),
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: None,
        };
        let config = Config::load(&args).unwrap();
        assert_eq!(config.port, 9090);
        assert_eq!(config.data, PathBuf::from("/tmp/hb"));
        assert_eq!(config.max_hooks, 50);
        // Remaining fields are defaults
        assert_eq!(config.max_payload, 1_048_576);
        assert_eq!(config.retention, 86_400);
        assert_eq!(config.rate_limit, 60);
        assert_eq!(config.max_requests, 1000);
    }

    // AC-3: CLI flag wins over TOML
    #[test]
    fn cli_overrides_toml() {
        let mut tmpfile = tempfile().unwrap();
        writeln!(tmpfile.1, "port = 7070").unwrap();

        let args = ServeArgs {
            port: Some(9090),
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: Some(tmpfile.0),
        };
        let config = Config::load(&args).unwrap();
        assert_eq!(config.port, 9090);
    }

    // AC-4: TOML value used when no CLI override
    #[test]
    fn toml_applies_when_no_cli_override() {
        let mut tmpfile = tempfile().unwrap();
        writeln!(tmpfile.1, "max_hooks = 200").unwrap();

        let args = ServeArgs {
            port: None,
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: Some(tmpfile.0),
        };
        let config = Config::load(&args).unwrap();
        assert_eq!(config.max_hooks, 200);
        // Other fields remain defaults
        assert_eq!(config.port, 3000);
    }

    // AC-5: Invalid TOML → error
    #[test]
    fn invalid_toml_returns_error() {
        let mut tmpfile = tempfile().unwrap();
        writeln!(tmpfile.1, "this is not = [valid toml {{{{").unwrap();

        let args = ServeArgs {
            port: None,
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: Some(tmpfile.0),
        };
        let err = Config::load(&args).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid TOML"), "got: {msg}");
    }

    // AC-6: Missing config file → error
    #[test]
    fn missing_config_file_returns_error() {
        let args = ServeArgs {
            port: None,
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: Some(PathBuf::from("/nonexistent/config.toml")),
        };
        let err = Config::load(&args).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("cannot read config file"), "got: {msg}");
        assert!(msg.contains("/nonexistent/config.toml"), "got: {msg}");
    }

    // AC-7: Full layering — defaults + TOML + CLI
    #[test]
    fn full_layering_defaults_toml_cli() {
        let mut tmpfile = tempfile().unwrap();
        writeln!(tmpfile.1, "port = 7070\nmax_hooks = 200\nretention = 3600").unwrap();

        let args = ServeArgs {
            port: None,            // TOML wins: 7070
            data: None,            // default: ./hookbin-data
            max_hooks: Some(50),   // CLI wins over TOML: 50
            max_payload: None,     // default: 1048576
            retention: None,       // TOML wins: 3600
            rate_limit: Some(120), // CLI wins over default: 120
            max_requests: None,    // default: 1000
            config: Some(tmpfile.0),
        };
        let config = Config::load(&args).unwrap();
        assert_eq!(config.port, 7070);
        assert_eq!(config.data, PathBuf::from("./hookbin-data"));
        assert_eq!(config.max_hooks, 50);
        assert_eq!(config.max_payload, 1_048_576);
        assert_eq!(config.retention, 3600);
        assert_eq!(config.rate_limit, 120);
        assert_eq!(config.max_requests, 1000);
    }

    // Unknown TOML keys are rejected
    #[test]
    fn unknown_toml_key_returns_error() {
        let mut tmpfile = tempfile().unwrap();
        writeln!(tmpfile.1, "unknown_key = true").unwrap();

        let args = ServeArgs {
            port: None,
            data: None,
            max_hooks: None,
            max_payload: None,
            retention: None,
            rate_limit: None,
            max_requests: None,
            config: Some(tmpfile.0),
        };
        let err = Config::load(&args).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid TOML"), "got: {msg}");
    }

    /// Helper: create a named temp file that persists for the test.
    fn tempfile() -> Result<(PathBuf, std::fs::File), std::io::Error> {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("hookbin-test-{}.toml", nanoid::nanoid!()));
        let file = std::fs::File::create(&path)?;
        Ok((path, file))
    }
}
