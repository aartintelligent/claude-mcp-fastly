//! Layered application configuration.
//!
//! Configuration values are resolved at startup by [`Config::load`] from the
//! sources below, with later sources overriding earlier ones:
//!
//! 1. Compiled-in defaults (`127.0.0.1:8000`).
//! 2. `/etc/<crate-name>/config.json` when present (path derived from
//!    [`PROJECT_NAME`]).
//! 3. The process environment, including any variables loaded from a `.env`
//!    file by `dotenvy::dotenv`. Variables are expected to use the `APP_`
//!    prefix and the `__` (double underscore) separator for nested fields,
//!    e.g. `APP_SERVER__HOST`.

use std::net::{IpAddr, SocketAddr};

use config::{Config as ConfigBuilder, Environment, File, FileFormat};
use serde::Deserialize;

use crate::error::Result;

/// Crate name as declared in `Cargo.toml`, captured at compile time via
/// `env!("CARGO_PKG_NAME")`. Drives the system-config lookup path so renaming
/// the package automatically retargets it without any extra wiring.
const PROJECT_NAME: &str = env!("CARGO_PKG_NAME");

/// Fully-resolved runtime configuration.
///
/// Populated once at startup and treated as immutable thereafter. Hot
/// reloading is intentionally not supported: the daemon is expected to be
/// restarted by its orchestrator (e.g. `docker restart`) when configuration
/// changes.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// HTTP server bind parameters.
    pub server: ServerConfig,

    /// Credentials and endpoint used to talk to the Fastly management API.
    pub fastly: FastlyConfig,
}

/// HTTP server bind parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// IP address the HTTP server binds to. Use `0.0.0.0` to listen on every
    /// interface (typical inside a Docker container).
    pub host: IpAddr,

    /// TCP port the HTTP server listens on.
    pub port: u16,
}

/// Base URL of the Fastly management API.
///
/// Hardcoded on purpose: there is no operational reason to point this server
/// at anything else, and exposing it as a config knob would only invite
/// misconfiguration. The token is sent on every request via the `Fastly-Key`
/// header by [`fastly_api`].
pub const FASTLY_BASE_URL: &str = "https://api.fastly.com";

/// Fastly API credentials.
#[derive(Debug, Clone, Deserialize)]
pub struct FastlyConfig {
    /// Fastly API token. Required — the server fails to start when absent.
    /// Set via `APP_FASTLY__API_TOKEN`.
    pub api_token: String,
}

impl ServerConfig {
    /// Returns the [`SocketAddr`] composed of [`ServerConfig::host`] and
    /// [`ServerConfig::port`].
    #[must_use]
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

impl Config {
    /// Loads the configuration from the layered sources documented at the
    /// module level.
    ///
    /// Reading any single source is best-effort: missing files do not error
    /// out and a missing `.env` is silently ignored. Only structural
    /// problems — malformed JSON, type-incompatible env vars, missing
    /// mandatory fields — surface as errors.
    ///
    /// # Errors
    ///
    /// Returns an error if a source cannot be parsed or if the merged
    /// configuration cannot be deserialized into [`Config`].
    pub fn load() -> Result<Self> {
        let _ = dotenvy::dotenv();

        let cfg = ConfigBuilder::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 8000)?
            .add_source(
                File::with_name(&format!("/etc/{PROJECT_NAME}/config.json"))
                    .format(FileFormat::Json)
                    .required(false),
            )
            .add_source(
                Environment::with_prefix("APP")
                    .prefix_separator("_")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;

        Ok(cfg.try_deserialize()?)
    }
}
