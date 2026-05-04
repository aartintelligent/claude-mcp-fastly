//! HTTP layer mounting the MCP service onto an `axum` router.
//!
//! The MCP protocol travels over the streamable-HTTP transport described by
//! the [MCP specification][spec]. Sessions are tracked in-process by an
//! [`rmcp::transport::streamable_http_server::session::local::LocalSessionManager`].
//!
//! [spec]: https://spec.modelcontextprotocol.io/specification/2024-11-05/

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;

use crate::app::AppState;
use crate::mcp::handler::Handler;

/// Maximum duration a session may stay idle before the server tears it down.
///
/// The rmcp default is 5 minutes, which is short enough to surface
/// `keep alive timeout` errors when leaving the MCP Inspector open without
/// activity. 30 minutes is more forgiving for interactive use without
/// retaining truly abandoned sessions indefinitely.
const SESSION_KEEP_ALIVE: Duration = Duration::from_mins(30);

/// Builds the `Router<AppState>` that serves the MCP endpoint at `/mcp`.
///
/// A fresh [`Handler`] is constructed for every session through the factory
/// closure. The supplied [`CancellationToken`] is forwarded to the
/// [`StreamableHttpServerConfig`] so the transport interrupts in-flight
/// streams as soon as the host application initiates shutdown.
pub fn router(state: &AppState, ct: CancellationToken) -> Router<AppState> {
    let mcp_state = state.clone();

    let mut session_manager = LocalSessionManager::default();

    session_manager.session_config.keep_alive = Some(SESSION_KEEP_ALIVE);

    let svc = StreamableHttpService::new(
        move || Ok(Handler::new(mcp_state.clone())),
        Arc::new(session_manager),
        StreamableHttpServerConfig::default().with_cancellation_token(ct),
    );

    Router::<AppState>::new().nest_service("/mcp", svc)
}
