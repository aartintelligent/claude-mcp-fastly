//! Graceful-shutdown signal coordination.
//!
//! Awaits the first incoming termination signal then cancels the supplied
//! [`CancellationToken`], allowing in-flight MCP sessions and the axum
//! accept-loop to drain cooperatively.

use tokio_util::sync::CancellationToken;

/// Awaits the first incoming shutdown signal, then cancels `ct`.
///
/// Listens to `SIGINT` (Ctrl-C) on every platform and to `SIGTERM` on Unix
/// targets — the signal sent by `docker stop`. On non-Unix platforms the
/// SIGTERM branch resolves to a future that never completes, so only Ctrl-C
/// will fire.
///
/// If a handler fails to register (e.g. a sandboxed environment without the
/// expected `signalfd` access) a warning is emitted and that branch is
/// replaced by a future that never resolves, so it cannot win the
/// [`tokio::select!`] race and trigger an immediate shutdown at startup.
pub async fn wait(ct: CancellationToken) {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::warn!(error = %e, "failed to install Ctrl-C handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }

    tracing::info!("shutdown signal received");
    ct.cancel();
}
