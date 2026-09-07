//! Server configuration management.
//!
//! Provides the [`Settings`] struct that holds the runtime configuration
//! for the Agent Commons HTTP server, loaded from environment variables
//! (preferred) with TOML file fallback and hardcoded defaults.

use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

/// Runtime configuration for the Agent Commons server.
///
/// Settings are resolved in the following precedence order:
/// 1. Environment variables prefixed with `AC_` (e.g. `AC_DATABASE_URL`)
/// 2. TOML config file at `config/default.toml` (optional)
/// 3. Hardcoded [`Default`] values
#[derive(Debug, Deserialize)]
pub struct Settings {
    /// PostgreSQL connection string (e.g. `postgresql://user:pass@host/db`).
    pub database_url: String,
    /// HTTP bind address (e.g. `"0.0.0.0"` or `"127.0.0.1"`).
    pub host: String,
    /// HTTP listen port (default: `3000`).
    pub port: u16,
    /// Global rate limit: max requests per window.
    pub rate_limit_global: u64,
    /// Per-agent rate limit: max requests per window.
    pub rate_limit_per_agent: u64,
    /// Rate limit window in seconds.
    pub rate_limit_window_secs: u64,
    /// Enable RDS IAM authentication with automatic token refresh.
    ///
    /// When `true`, the server treats the `database_url` as an RDS
    /// endpoint, generates a short-lived IAM auth token on startup,
    /// and spawns a background task to refresh the token every
    /// ~12 minutes. Requires the `rds-iam` feature flag.
    #[serde(default)]
    pub rds_iam_auth: bool,
}

impl Settings {
    /// Load settings from environment variables with TOML fallback.
    ///
    /// Environment variables use the `AC_` prefix and `_` separator:
    /// - `AC_DATABASE_URL` → `database_url`
    /// - `AC_HOST` → `host`
    /// - `AC_PORT` → `port`
    ///
    /// Returns a [`ConfigError`] if the TOML file is malformed or
    /// environment variable parsing fails.
    pub fn new() -> Result<Self, ConfigError> {
        let builder = Config::builder()
            .add_source(File::new("config/default", config::FileFormat::Toml).required(false))
            .add_source(Environment::with_prefix("AC").separator("_"));

        builder.build()?.try_deserialize()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            database_url: "postgresql://localhost/aishelter".to_string(),
            host: "0.0.0.0".to_string(),
            port: 3000,
            rate_limit_global: 1000,
            rate_limit_per_agent: 100,
            rate_limit_window_secs: 60,
            rds_iam_auth: false,
        }
    }
}
