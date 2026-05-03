//! Model Context Protocol server implementation.
//!
//! The module groups the four primitives exposed over MCP — [`tools`],
//! [`tasks`], [`prompts`], [`resources`] — alongside the `ServerHandler`
//! implementation that ties them together (in [`handler`]) and the
//! `axum`-aware [`router`] constructor used by the binary.
//!
//! Wire-level concerns (transport, sessions, streaming) are delegated to
//! [`rmcp`].

mod handler;
pub mod prompts;
pub mod resources;
mod server;
pub mod tasks;
pub mod tools;

pub use server::router;
