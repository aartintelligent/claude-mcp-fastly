//! Trivial greeting tool used to validate the MCP wiring end-to-end.

use rmcp::{ErrorData as McpError, model::*};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::app::AppState;

/// Arguments accepted by the `hello` tool.
///
/// Both the rmcp request decoder and the JSON schema advertised on
/// `tools/list` are derived from this struct.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct HelloArgs {
    /// Optional name to greet. Defaults to `"world"` when absent or `null`.
    #[serde(default)]
    pub name: Option<String>,
}

/// Returns a greeting wrapped in a successful [`CallToolResult`].
///
/// # Errors
///
/// Currently infallible. The [`Result`] return type matches the contract
/// expected by the rmcp tool router and reserves room for future fallible
/// behavior without breaking call sites.
pub async fn run(_state: &AppState, args: HelloArgs) -> Result<CallToolResult, McpError> {
    let who = args.name.as_deref().unwrap_or("world");
    Ok(CallToolResult::success(vec![Content::text(format!(
        "Hello, {who}!"
    ))]))
}
