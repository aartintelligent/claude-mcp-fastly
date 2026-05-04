//! `list_resource_kv_stores` tool: list the Fastly account's KV stores.
//!
//! KV stores are an **account-scoped** resource designed for high-volume
//! key/value storage (potentially millions of items per store). Unlike
//! edge dictionaries and config stores, the Fastly API does *not* expose
//! an `info` endpoint for KV stores — there is no `item_count` to merge
//! at catalog time, only identity + timestamps.
//!
//! Pagination is cursor-based. Pass `next_cursor` from a previous response
//! back as `cursor` to fetch the next page; when `next_cursor` is absent
//! from the response, the catalog is exhausted.
//!
//! For the items themselves, see [`super::list_resource_kv_store_items`] —
//! note that it returns **keys only**, not key/value pairs (Fastly's KV
//! API forbids bulk listing of values).

use fastly_api::apis::kv_store_api::{KvStoreListParams, kv_store_list};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `list_resource_kv_stores` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListResourceKvStoresArgs {
    /// Optional exact-name filter forwarded to Fastly. When set, the
    /// upstream response is restricted to a single store (or empty).
    #[serde(default)]
    pub name: Option<String>,
    /// Optional cursor for pagination — pass the `next_cursor` returned
    /// by a previous call to retrieve the next page.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Optional page size. When unset, Fastly applies its own default.
    #[serde(default)]
    pub limit: Option<i32>,
}

/// Slimmed-down view of a Fastly KV store entry.
#[derive(Serialize)]
struct KvStoreSummary<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Wrapper carrying the page of KV stores plus the pagination cursor for
/// subsequent calls.
#[derive(Serialize)]
struct ListKvStoresResponse<'a> {
    stores: Vec<KvStoreSummary<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_cursor: Option<&'a str>,
}

/// Returns a page of KV-store summaries plus the cursor for the next page
/// (if any).
///
/// # Errors
///
/// Returns an MCP internal error if the Fastly call fails (network, auth,
/// deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: ListResourceKvStoresArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let response = kv_store_list(
        &mut cfg,
        KvStoreListParams {
            name: args.name.clone(),
            cursor: args.cursor.clone(),
            limit: args.limit,
        },
    )
    .await
    .map_err(|e| McpError::internal_error(format!("Fastly kv_store_list failed: {e}"), None))?;

    let stores: Vec<KvStoreSummary> = response
        .data
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|s| KvStoreSummary {
            id: s.id.as_deref(),
            name: s.name.as_deref(),
            created_at: s.created_at.as_deref(),
            updated_at: s.updated_at.as_deref(),
        })
        .collect();

    let next_cursor = response
        .meta
        .as_deref()
        .and_then(|m| m.next_cursor.as_deref());

    Ok(CallToolResult::success(vec![Content::json(
        &ListKvStoresResponse {
            stores,
            next_cursor,
        },
    )?]))
}
