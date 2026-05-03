//! Long-running tools designed for task-augmented invocation.
//!
//! Tools in this module are conventionally invoked by clients with a `task`
//! field on `tools/call`, which makes the server return a `CreateTaskResult`
//! (containing a `taskId`) immediately and execute the work in the
//! background. Clients then poll `tasks/get` and retrieve the final value
//! via `tasks/result`.
//!
//! See the MCP specification (revision `2025-11-25`) for the protocol
//! details, and [`crate::mcp::handler`] for the wiring (`#[task_handler]`
//! attribute + `OperationProcessor` field).
//!
//! Each module declares the same shape as a regular tool:
//! - a public `Args` struct deriving [`serde::Deserialize`] and
//!   [`schemars::JsonSchema`];
//! - a public async `run` function taking `(&AppState, Args)` and returning
//!   `Result<CallToolResult, McpError>`.
//!
//! Whether a given invocation actually goes through the task lifecycle is
//! decided by the client at call time.

pub mod slow_demo;
