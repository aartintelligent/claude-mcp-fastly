//! Model Context Protocol server implementation.
//!
//! The module exposes [`tools`] — currently the only MCP primitive enabled —
//! alongside the `ServerHandler` implementation that ties them to the rmcp
//! transport (in [`handler`]) and the `axum`-aware [`router`] constructor
//! used by the binary.
//!
//! Wire-level concerns (transport, sessions, streaming) are delegated to
//! [`rmcp`].

mod handler;
mod server;
pub mod tools;

pub use server::router;
