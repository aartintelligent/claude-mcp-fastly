//! Fake long-running tool used to validate the task-augmented call path
//! end-to-end.
//!
//! Sleeps for the configured number of seconds (default 15) and returns a
//! short text confirmation. With `#[task_handler]` enabled on the
//! [`crate::mcp::handler::Handler`], a client may invoke this tool with a
//! `task` field and poll the result via `tasks/get` / `tasks/result`.

use std::time::Duration;

use rmcp::{ErrorData as McpError, model::*};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::AppState;

/// Default sleep duration when [`SlowDemoArgs::seconds`] is unset.
const DEFAULT_SECONDS: u64 = 15;

/// Arguments accepted by the `slow_demo` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SlowDemoArgs {
    /// Optional sleep duration in seconds. Defaults to 15.
    #[serde(default)]
    pub seconds: Option<u64>,
}

/// Sleeps `seconds` seconds (15 by default), then returns a confirmation.
///
/// # Errors
///
/// Currently infallible. The [`Result`] return type matches the contract
/// expected by the rmcp tool router.
pub async fn run(_state: &AppState, args: SlowDemoArgs) -> Result<CallToolResult, McpError> {
    let seconds = args.seconds.unwrap_or(DEFAULT_SECONDS);

    tracing::info!(seconds, "slow_demo started");
    tokio::time::sleep(Duration::from_secs(seconds)).await;
    tracing::info!(seconds, "slow_demo completed");

    Ok(CallToolResult::success(vec![Content::text(format!(
        "Slow demo completed after {seconds}s"
    ))]))
}
