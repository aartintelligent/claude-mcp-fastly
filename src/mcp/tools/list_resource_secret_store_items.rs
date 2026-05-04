//! `list_resource_secret_store_items` tool: list the secrets in a single
//! Fastly secret store.
//!
//! Backed by `GET /resources/stores/secret/{store_id}/secrets`.
//!
//! **Important contract**: secret *values* are never returned by the
//! Fastly API — by design, secret stores expose only an opaque `digest`
//! (useful to detect rotations) plus the secret `name` and creation
//! timestamp. The plaintext is reachable only at runtime from VCL or
//! Compute, never via management. This MCP cannot bypass that contract;
//! there is therefore no `get_resource_secret_store_item_value` tool.
//!
//! Pagination is cursor-based; pass `next_cursor` from a previous response
//! back as `cursor` to retrieve the next page.

use fastly_api::apis::Error;
use fastly_api::apis::secret_store_item_api::{GetSecretsParams, get_secrets};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_resource_secret_store_items` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceSecretStoreItemsArgs {
    /// Alphanumeric Fastly secret store identifier — typically obtained
    /// from `list_resource_secret_stores`'s `id` field.
    pub store_id: String,
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

/// One secret listing — name, opaque digest, and creation timestamp.
/// **Never includes the value.**
#[derive(Serialize)]
struct SecretSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    digest: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
}

/// Wrapper carrying the page of secret entries plus the pagination cursor
/// for subsequent calls.
#[derive(Serialize)]
struct ListSecretStoreItemsResponse<'a> {
    items: Vec<SecretSummary<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<&'a str>,
}

/// Returns a page of secret summaries (name + digest + creation
/// timestamp) for the given store. Values are not — and cannot be —
/// returned: the Fastly secret store API simply does not expose them.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// (unknown store id).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListResourceSecretStoreItemsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = match get_secrets(
        &mut cfg,
        GetSecretsParams {
            store_id: args.store_id.clone(),
            cursor: args.cursor.clone(),
            limit: args.limit.map(|n| n.to_string()),
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No secret store found with id `{}`.",
                args.store_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly get_secrets failed: {e}"),
                None,
            ));
        }
    };

    let items: Vec<SecretSummary> = response
        .data
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|s| SecretSummary {
            name: s.name.as_deref(),
            digest: s.digest.as_deref(),
            created_at: s.created_at.as_deref(),
        })
        .collect();

    let next_cursor = response
        .meta
        .as_deref()
        .and_then(|m| m.next_cursor.as_deref());

    Ok(CallToolResult::success(vec![Content::json(
        &ListSecretStoreItemsResponse { items, next_cursor },
    )?]))
}
