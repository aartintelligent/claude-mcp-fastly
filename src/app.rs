//! Shared application state.
//!
//! The state container is cloned for every `axum` handler invocation, so
//! every field must be cheap to clone. This is currently achieved by holding
//! the immutable [`Config`] and the prebuilt Fastly API [`FastlyConfiguration`]
//! behind [`Arc`]s.

use std::net::SocketAddr;
use std::sync::Arc;

use fastly_api::apis::configuration::{
    ApiKey, Configuration as FastlyConfiguration,
};

use crate::config::{Config, FastlyConfig};

/// Read-only services and configuration shared across request handlers.
///
/// Attached to the router through [`axum::Router::with_state`] and extracted
/// inside handlers via `axum::extract::State<AppState>`. Cloning is a single
/// atomic increment thanks to the inner [`Arc`]s.
///
/// # Extending
///
/// Additional services should be added as `Arc`-wrapped fields rather than
/// owning values, so cloning the state remains a constant-time operation
/// regardless of the inner data size.
#[derive(Clone)]
pub struct AppState {
    /// Resolved runtime configuration. Held behind an [`Arc`] to keep
    /// [`AppState`] cheap to clone as the configuration grows.
    pub config: Arc<Config>,

    /// Pre-built Fastly API client configuration (token, base URL, shared
    /// `reqwest::Client`). Per-call code obtains an owned, mutable clone via
    /// [`AppState::fastly_config`] because every endpoint in [`fastly_api`]
    /// takes `&mut Configuration` to update rate-limit counters from the
    /// response headers.
    fastly: Arc<FastlyConfiguration>,
}

impl AppState {
    /// Constructs a new state from the supplied [`Config`].
    ///
    /// The configuration is moved into a fresh [`Arc`]; the Fastly API client
    /// configuration is built once from [`Config::fastly`] so that every tool
    /// invocation reuses the same `reqwest::Client` (and its connection pool)
    /// rather than spinning a fresh one per call.
    #[must_use]
    pub fn new(config: Config) -> Self {
        let fastly = build_fastly_configuration(&config.fastly);
        Self {
            config: Arc::new(config),
            fastly: Arc::new(fastly),
        }
    }

    /// Returns the [`SocketAddr`] the HTTP server should bind to.
    ///
    /// Convenience wrapper around [`crate::config::ServerConfig::bind_addr`].
    #[must_use]
    pub fn bind_addr(&self) -> SocketAddr {
        self.config.server.bind_addr()
    }

    /// Returns an owned [`FastlyConfiguration`] ready to be passed to any
    /// [`fastly_api`] endpoint.
    ///
    /// Cloning is cheap: the inner `reqwest::Client` is `Arc`-backed, so the
    /// connection pool is shared across calls. We hand out an owned value
    /// (rather than a reference) because every endpoint takes
    /// `&mut Configuration` to update its rate-limit fields.
    #[must_use]
    pub fn fastly_config(&self) -> FastlyConfiguration {
        (*self.fastly).clone()
    }
}

/// Maps our [`FastlyConfig`] onto the [`fastly_api`] client configuration.
///
/// The base URL flows through [`FastlyConfig::base_url`] (defaulted by
/// [`crate::config::Config::load`] so callers don't have to set it).
/// `..Default::default()` reuses the upstream defaults for fields we do not
/// expose (user agent, rate-limit counters, the freshly-allocated
/// `reqwest::Client`). The api_key field is always overridden, so the
/// upstream `FASTLY_API_TOKEN` env-var fallback baked into the default impl
/// is never observed in practice.
fn build_fastly_configuration(cfg: &FastlyConfig) -> FastlyConfiguration {
    FastlyConfiguration {
        base_path: cfg.base_url.clone(),
        api_key: Some(ApiKey {
            prefix: None,
            key: cfg.api_token.clone(),
        }),
        ..Default::default()
    }
}
