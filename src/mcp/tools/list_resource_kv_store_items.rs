//! `list_resource_kv_store_items` tool: list the keys of a single Fastly
//! KV store.
//!
//! **Important contract**: this tool returns **keys only**, not key/value
//! pairs. Fastly's KV stores are designed for high-volume storage and
//! deliberately offer no bulk-listing of values — each value must be
//! fetched individually with [`super::get_resource_kv_store_item_value`].
//!
//! Pagination is cursor-based; pass `next_cursor` from a previous response
//! back as `cursor`. An optional `prefix` lets the agent ask Fastly to
//! only return keys starting with a given string, server-side.

use fastly_api::apis::Error;
use fastly_api::apis::kv_store_item_api::{
    KvStoreListItemKeysParams, kv_store_list_item_keys,
};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_resource_kv_store_items` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceKvStoreItemsArgs {
    /// Alphanumeric Fastly KV store identifier — typically obtained from
    /// `list_resource_kv_stores`'s `id` field.
    pub store_id: String,
    /// Optional key-prefix filter forwarded to Fastly.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Optional cursor for pagination — pass the `next_cursor` returned
    /// by a previous call to retrieve the next page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Optional page size. When unset, Fastly applies its own default.
    #[serde(default)]
    pub limit: Option<i32>,
}

/// Wrapper carrying a page of keys plus the pagination cursor for the
/// next page.
#[derive(Serialize)]
struct ListKvStoreItemsResponse<'a> {
    keys: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<&'a str>,
}

/// Returns a page of keys for the given KV store.
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
    args: ListResourceKvStoreItemsArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = match kv_store_list_item_keys(
        &mut cfg,
        KvStoreListItemKeysParams {
            store_id: args.store_id.clone(),
            cursor: args.cursor.clone(),
            limit: args.limit,
            prefix: args.prefix.clone(),
            consistency: None,
        },
    )
    .await
    {
        Ok(r) => r,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No KV store found with id `{}`.",
                args.store_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly kv_store_list_item_keys failed: {e}"),
                None,
            ));
        }
    };

    let keys = response.data.as_deref().unwrap_or_default();
    let next_cursor = response
        .meta
        .as_deref()
        .and_then(|m| m.next_cursor.as_deref());

    Ok(CallToolResult::success(vec![Content::json(
        &ListKvStoreItemsResponse { keys, next_cursor },
    )?]))
}
