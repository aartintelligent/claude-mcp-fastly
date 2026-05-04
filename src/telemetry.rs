//! Tracing subscriber bootstrap.
//!
//! Installs the global [`tracing`] subscriber from the `RUST_LOG`
//! environment variable, falling back to `info` when unset.

use tracing_subscriber::{EnvFilter, fmt};

use crate::error;

/// Installs the global tracing subscriber.
///
/// Filter directives are read from the `RUST_LOG` environment variable and
/// fall back to `info` when unset. Intended to be called exactly once during
/// bootstrap.
///
/// # Errors
///
/// Returns an error if a global subscriber has already been installed in
/// this process — the only failure mode of
/// [`tracing_subscriber::util::SubscriberInitExt::try_init`].
pub fn init() -> error::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

    fmt()
        .with_env_filter(filter)
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to install tracing subscriber: {e}").into())
}
