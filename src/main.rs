//! Binary entry point.
//!
//! Boots the [`app::AppState`] from configuration, builds the HTTP router
//! exposing the MCP service (and any future REST endpoints), then serves
//! traffic until a shutdown signal is received.
//!
//! # Configuration sources
//!
//! Resolved by [`config::Config::load`] in the following order, with later
//! sources overriding earlier ones:
//!
//! 1. Built-in defaults (`127.0.0.1:8000`).
//! 2. `/etc/<PROJECT_NAME>/config.json` if present (resolved from
//!    `CARGO_PKG_NAME`, currently `claude-mcp-fastly` — typically a Docker
//!    volume or package install).
//! 3. A `.env` file in the working directory, loaded by `dotenvy::dotenv`.
//! 4. Process environment variables prefixed by `APP_`, with `__` separating
//!    nested fields (e.g. `APP_SERVER__HOST=0.0.0.0`).
//!
//! # Graceful shutdown
//!
//! The server reacts to `SIGINT` (Ctrl-C) on every platform and to `SIGTERM`
//! on Unix targets — the signal sent by `docker stop`. Upon receiving either
//! signal the [`CancellationToken`] is canceled and `axum`'s graceful
//! shutdown pipeline drains in-flight requests before exiting.

mod app;
mod config;
mod error;
mod mcp;
mod shutdown;
mod telemetry;

use std::net::SocketAddr;

use axum::Router;
use tokio_util::sync::CancellationToken;

/// Process entry point.
///
/// Initializes the global tracing subscriber, loads the [`config::Config`],
/// constructs the shared [`app::AppState`], composes the HTTP router, and
/// hands it to [`serve`]. A returned error is printed by the standard
/// library's main-error handler and yields a non-zero exit code.
///
/// # Errors
///
/// Returns an error if tracing initialization fails, configuration loading
/// fails, the listener cannot bind to the configured address, or the server
/// terminates abnormally.
#[tokio::main]
async fn main() -> error::Result<()> {
    telemetry::init()?;

    let config = config::Config::load()?;

    let state = app::AppState::new(config);

    let addr = state.bind_addr();

    let ct = CancellationToken::new();

    let router = build_router(state, ct.clone());

    serve(router, addr, ct).await
}

/// Composes the top-level [`axum::Router`] from every feature-area sub-router.
///
/// This is the single seam where additional REST endpoints are mounted
/// alongside the MCP service. New sub-routers should be merged here and stay
/// in the `Router<AppState>` shape so that [`Router::with_state`] can attach
/// the shared state in one place.
fn build_router(state: app::AppState, ct: CancellationToken) -> Router {
    Router::new()
        .merge(mcp::router(&state, ct))
        .with_state(state)
}

/// Binds a TCP listener and runs the HTTP server until shutdown.
///
/// Uses `axum::serve` with a graceful-shutdown future that resolves on the
/// first received signal. The [`CancellationToken`] is wired through the
/// server stack — notably the MCP transport — so long-lived sessions wind
/// down cleanly before the process exits.
///
/// # Errors
///
/// Returns an error if the listener cannot bind, if the local address cannot
/// be queried, or if the accept loop terminates with an I/O error.
async fn serve(router: Router, addr: SocketAddr, ct: CancellationToken) -> error::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let actual = listener.local_addr()?;

    tracing::info!(addr = %actual, "server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown::wait(ct))
        .await?;

    Ok(())
}
