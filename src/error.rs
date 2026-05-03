//! Crate-wide error type and result alias.
//!
//! Every fallible operation in the crate funnels through [`AppError`]. The
//! enum collapses the categories the application is expected to distinguish
//! operationally — I/O, configuration, and an untyped catch-all — while
//! preserving the originating error's display and source chain through
//! `#[error(transparent)]`.

use thiserror::Error;

/// Crate-local convenience alias.
///
/// Defaults to [`AppError`] for the error type, mirroring the shape of
/// `std::io::Result` and similar typedefs from the standard library.
pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// Top-level error returned by the binary.
///
/// Variants are intentionally broad. `#[error(transparent)]` forwards the
/// inner error's [`Display`](std::fmt::Display) and
/// [`source`](std::error::Error::source) implementations verbatim, which
/// keeps the original context available for diagnostics.
#[derive(Debug, Error)]
pub enum AppError {
    /// Wraps any [`std::io::Error`], typically raised by listener binding,
    /// signal handling, or filesystem access during configuration loading.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Configuration parsing or merging failure from the [`config`] crate.
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    /// Catch-all wrapper around [`anyhow::Error`] for error sources that do
    /// not warrant a dedicated variant.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
