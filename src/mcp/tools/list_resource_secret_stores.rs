//! `list_resource_secret_stores` tool: list the Fastly account's secret
//! stores.
//!
//! Secret stores are an **account-scoped** resource designed to hold
//! credentials and other sensitive material that VCL/Compute services
//! consume at runtime. Unlike config stores and KV stores, the secret
//! store API never returns the secret values themselves — only their
//! identity, an opaque digest, and timestamps. That contract is enforced
//! at the *API layer*, not just by a `write_only` flag, so there is no
//! way for any management-token holder (including this MCP server) to
//! exfiltrate the plaintext.
//!
//! This tool returns only the catalog of stores. To inspect the secrets
//! inside, see [`super::list_resource_secret_store_items`].
//!
//! Pagination is cursor-based; pass `next_cursor` from a previous
//! response back as `cursor` to retrieve the next page.

use fastly_api::apis::secret_store_api::{GetSecretStoresParams, get_secret_stores};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_resource_secret_stores` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceSecretStoresArgs {
    /// Optional exact-name filter forwarded to Fastly. When set, the
    /// upstream response is restricted to a single store (or empty).
    #[serde(default)]
    pub name: Option<String>,
    /// Optional cursor for pagination — pass the `next_cursor` returned
    /// by a previous call to retrieve the next page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Optional page size (max 200). Fastly accepts this as a string on
    /// the wire; we take an `i32` here for ergonomics and convert
    /// internally.
    #[serde(default)]
    pub limit: Option<i32>,
}

/// Slimmed-down view of a Fastly secret-store entry. Mirrors the upstream
/// `SecretStoreResponse` shape verbatim — there is little to drop, since
/// secret-store metadata is intentionally minimal.
#[derive(Serialize)]
struct SecretStoreSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
}

/// Wrapper carrying the page of secret stores plus the pagination cursor
/// for subsequent calls.
#[derive(Serialize)]
struct ListSecretStoresResponse<'a> {
    stores: Vec<SecretStoreSummary<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<&'a str>,
}

/// Returns a page of secret-store summaries plus the cursor for the next
/// page (if any).
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails (network, auth,
/// deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListResourceSecretStoresArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = get_secret_stores(
        &mut cfg,
        GetSecretStoresParams {
            name: args.name.clone(),
            cursor: args.cursor.clone(),
            limit: args.limit.map(|n| n.to_string()),
        },
    )
    .await
    .map_err(|e| McpError::internal_error(format!("Fastly get_secret_stores failed: {e}"), None))?;

    let stores: Vec<SecretStoreSummary> = response
        .data
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|s| SecretStoreSummary {
            id: s.id.as_deref(),
            name: s.name.as_deref(),
            created_at: s.created_at.as_deref(),
        })
        .collect();

    let next_cursor = response
        .meta
        .as_deref()
        .and_then(|m| m.next_cursor.as_deref());

    Ok(CallToolResult::success(vec![Content::json(
        &ListSecretStoresResponse {
            stores,
            next_cursor,
        },
    )?]))
}
