//! `get_resource_config_store_item_value` tool: fetch the value
//! associated with a single key in a Fastly config store.
//!
//! Backed by `config_store_item_api::get_config_store_item`, which maps
//! to `GET /resources/stores/config/{config_store_id}/item/{key}`. The
//! SDK works cleanly here (the response is a properly-shaped
//! `ConfigStoreItemResponse` JSON object) — no raw-HTTP bypass needed,
//! unlike the KV equivalent.
//!
//! Returns a flat projection `{ config_store_id, key, item_value,
//! created_at, updated_at }`. Drops the upstream `store_id` (caller-known)
//! and `deleted_at` (operational noise).

use fastly_api::apis::Error;
use fastly_api::apis::config_store_item_api::{
    GetConfigStoreItemParams, get_config_store_item,
};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, Content},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Arguments accepted by the `get_resource_config_store_item_value` tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetResourceConfigStoreItemValueArgs {
    /// Alphanumeric Fastly config store identifier — typically obtained
    /// from `list_resource_config_stores`'s `id` field.
    pub config_store_id: String,
    /// The key to read. Listed by `list_resource_config_store_items`
    /// (which returns keys only — values must be fetched one at a time
    /// via this tool).
    pub key: String,
}

/// Wrapper carrying the requested store/key alongside the value, so the
/// agent has the full context in a single JSON object.
#[derive(Serialize)]
struct ConfigStoreItemValue<'a> {
    config_store_id: &'a str,
    key: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_value: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at: Option<&'a str>,
}

/// Returns the value at `(config_store_id, key)`.
///
/// A `404` from Fastly is downgraded to a plain-text "not found" message
/// (covers both unknown store id and unknown key — Fastly does not
/// distinguish them at this endpoint).
///
/// # Errors
///
/// Returns an MCP internal error for any other Fastly failure (network,
/// auth, deserialization, 5xx).
pub async fn run(
    state: &AppState,
    args: GetResourceConfigStoreItemValueArgs,
) -> Result<CallToolResult, McpError> {
    let mut cfg = state.fastly_config();

    let item = match get_config_store_item(
        &mut cfg,
        GetConfigStoreItemParams {
            config_store_id: args.config_store_id.clone(),
            config_store_item_key: args.key.clone(),
        },
    )
    .await
    {
        Ok(i) => i,
        Err(Error::ResponseError(rc)) if rc.status.as_u16() == 404 => {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No value found for key `{}` in config store `{}`.",
                args.key, args.config_store_id
            ))]));
        }
        Err(e) => {
            return Err(McpError::internal_error(
                format!("Fastly get_config_store_item failed: {e}"),
                None,
            ));
        }
    };

    Ok(CallToolResult::success(vec![Content::json(
        &ConfigStoreItemValue {
            config_store_id: &args.config_store_id,
            key: &args.key,
            item_value: item.item_value.as_deref(),
            created_at: item.created_at.as_deref(),
            updated_at: item.updated_at.as_deref(),
        },
    )?]))
}
